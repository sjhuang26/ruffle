//! XML Tree structure

use crate::avm1::activation::Activation;
use crate::avm1::object::xml_attributes_object::XmlAttributesObject;
use crate::avm1::object::xml_node_object::XmlNodeObject;
use crate::avm1::property::Attribute;
use crate::avm1::{Object, ScriptObject, TObject};
use crate::string::{AvmString, WStr, WString};
use crate::xml;
use gc_arena::{Collect, GcCell, MutationContext};
use quick_xml::escape::escape;
use quick_xml::events::BytesStart;
use std::collections::BTreeMap;
use std::fmt;
use std::mem::swap;

const ELEMENT_NODE: u8 = 1;
const TEXT_NODE: u8 = 3;

/// Represents a node in the XML tree.
#[derive(Copy, Clone, Collect)]
#[collect(no_drop)]
pub struct XmlNode<'gc>(GcCell<'gc, XmlNodeData<'gc>>);

#[derive(Clone, Collect)]
#[collect(no_drop)]
pub struct XmlNodeData<'gc> {
    /// The script object associated with this XML node, if any.
    script_object: Option<Object<'gc>>,

    /// The script object associated with this XML node's attributes, if any.
    attributes_script_object: Option<Object<'gc>>,

    /// The parent node of this one.
    parent: Option<XmlNode<'gc>>,

    /// The previous sibling node to this one.
    prev_sibling: Option<XmlNode<'gc>>,

    /// The next sibling node to this one.
    next_sibling: Option<XmlNode<'gc>>,

    /// The type of this XML node. Should either `ELEMENT_NODE` or `TEXT_NODE`,
    /// but any other value is accepted as well.
    node_type: u8,

    /// The tag name of this element, or its text content, depending on `node_type`.
    /// None if this is a document root node.
    node_value: Option<AvmString<'gc>>,

    /// Attributes of the element.
    attributes: BTreeMap<AvmString<'gc>, AvmString<'gc>>,

    /// Child nodes of this element.
    children: Vec<XmlNode<'gc>>,
}

impl<'gc> XmlNode<'gc> {
    /// Construct a new XML text node.
    pub fn new_text(mc: MutationContext<'gc, '_>, contents: AvmString<'gc>) -> Self {
        Self(GcCell::allocate(
            mc,
            XmlNodeData {
                script_object: None,
                attributes_script_object: None,
                parent: None,
                prev_sibling: None,
                next_sibling: None,
                node_type: TEXT_NODE,
                node_value: Some(contents),
                attributes: BTreeMap::new(),
                children: Vec::new(),
            },
        ))
    }

    /// Construct a new XML element node.
    pub fn new_element(mc: MutationContext<'gc, '_>, element_name: AvmString<'gc>) -> Self {
        Self(GcCell::allocate(
            mc,
            XmlNodeData {
                script_object: None,
                attributes_script_object: None,
                parent: None,
                prev_sibling: None,
                next_sibling: None,
                node_type: ELEMENT_NODE,
                node_value: Some(element_name),
                attributes: BTreeMap::new(),
                children: Vec::new(),
            },
        ))
    }

    /// Construct a new XML root node.
    pub fn new_document_root(mc: MutationContext<'gc, '_>) -> Self {
        Self(GcCell::allocate(
            mc,
            XmlNodeData {
                script_object: None,
                parent: None,
                prev_sibling: None,
                next_sibling: None,
                node_type: ELEMENT_NODE,
                node_value: None,
                attributes: BTreeMap::new(),
                attributes_script_object: None,
                children: Vec::new(),
            },
        ))
    }

    /// Construct an XML Element node from a `quick_xml` `BytesStart` event.
    ///
    /// The returned node will always be an `Element`, and it must only contain
    /// valid encoded UTF-8 data. (Other encoding support is planned later.)
    pub fn from_start_event(
        activation: &mut Activation<'_, 'gc, '_>,
        bs: BytesStart<'_>,
        id_map: ScriptObject<'gc>,
    ) -> Result<Self, quick_xml::Error> {
        let tag_name = AvmString::new_utf8_bytes(activation.context.gc_context, bs.name())?;
        let mut node = Self::new_element(activation.context.gc_context, tag_name);
        for attribute in bs.attributes() {
            let attribute = attribute?;
            let key = AvmString::new_utf8_bytes(activation.context.gc_context, attribute.key)?;
            let value_bytes = attribute.unescaped_value()?;
            let value = AvmString::new_utf8_bytes(activation.context.gc_context, value_bytes)?;
            node.set_attribute_value(activation.context.gc_context, key, value);

            // Update the ID map.
            if attribute.key == b"id" {
                id_map.define_value(
                    activation.context.gc_context,
                    value,
                    node.script_object(activation).into(),
                    Attribute::empty(),
                );
            }
        }
        Ok(node)
    }

    /// Get the parent, if this node has one.
    pub fn parent(self) -> Option<XmlNode<'gc>> {
        self.0.read().parent
    }

    /// Get the previous sibling, if this node has one.
    pub fn prev_sibling(self) -> Option<XmlNode<'gc>> {
        self.0.read().prev_sibling
    }

    /// Set this node's previous sibling.
    fn set_prev_sibling(&mut self, mc: MutationContext<'gc, '_>, new_prev: Option<XmlNode<'gc>>) {
        self.0.write(mc).prev_sibling = new_prev;
    }

    /// Get the next sibling, if this node has one.
    pub fn next_sibling(self) -> Option<XmlNode<'gc>> {
        self.0.read().next_sibling
    }

    /// Set this node's next sibling.
    fn set_next_sibling(&mut self, mc: MutationContext<'gc, '_>, new_next: Option<XmlNode<'gc>>) {
        self.0.write(mc).next_sibling = new_next;
    }

    /// Remove node from its current siblings list.
    ///
    /// If a former sibling exists, we will also adopt it to the opposing side
    /// of this node, so as to maintain a coherent sibling list.
    ///
    /// This is the opposite of `adopt_siblings` - the former adds a node to a
    /// new sibling list, and this removes it from the current one.
    fn disown_siblings(&mut self, mc: MutationContext<'gc, '_>) {
        let old_prev = self.prev_sibling();
        let old_next = self.next_sibling();

        if let Some(mut prev) = old_prev {
            prev.set_next_sibling(mc, old_next);
        }

        if let Some(mut next) = old_next {
            next.set_prev_sibling(mc, old_prev);
        }

        self.set_prev_sibling(mc, None);
        self.set_next_sibling(mc, None);
    }

    /// Unset the parent of this node.
    fn disown_parent(&mut self, mc: MutationContext<'gc, '_>) {
        self.0.write(mc).parent = None;
    }

    /// Add node to a new siblings list.
    ///
    /// If a given sibling exists, we will also ensure this node is adopted as
    /// its sibling, so as to maintain a coherent sibling list.
    ///
    /// This is the opposite of `disown_siblings` - the former removes a
    /// sibling from its current list, and this adds the sibling to a new one.
    fn adopt_siblings(
        &mut self,
        mc: MutationContext<'gc, '_>,
        new_prev: Option<XmlNode<'gc>>,
        new_next: Option<XmlNode<'gc>>,
    ) {
        if let Some(mut prev) = new_prev {
            prev.set_next_sibling(mc, Some(*self));
        }

        if let Some(mut next) = new_next {
            next.set_prev_sibling(mc, Some(*self));
        }

        self.set_prev_sibling(mc, new_prev);
        self.set_next_sibling(mc, new_next);
    }

    /// Remove node from this node's child list.
    ///
    /// This function yields Err if this node cannot accept child nodes.
    fn orphan_child(&mut self, mc: MutationContext<'gc, '_>, child: XmlNode<'gc>) {
        if let Some(position) = self.child_position(child) {
            self.0.write(mc).children.remove(position);
        }
    }

    /// Insert `child` into the children list of this node.
    ///
    /// `child` will be adopted into the current tree: all child references
    /// to other nodes or documents will be adjusted to reflect its new
    /// position in the tree. This may remove it from any existing trees or
    /// documents.
    ///
    /// The `position` parameter is the position of the new child in
    /// this node's children list. This is used to find and link the child's
    /// siblings to each other.
    pub fn insert_child(
        &mut self,
        mc: MutationContext<'gc, '_>,
        position: usize,
        mut child: XmlNode<'gc>,
    ) {
        let is_cyclic = self
            .ancestors()
            .any(|ancestor| GcCell::ptr_eq(ancestor.0, child.0));
        if is_cyclic {
            return;
        }

        if let Some(mut old_parent) = child.0.read().parent {
            if !GcCell::ptr_eq(self.0, old_parent.0) {
                old_parent.orphan_child(mc, child);
            }
        }

        child.0.write(mc).parent = Some(*self);

        let children = &mut self.0.write(mc).children;
        children.insert(position, child);

        let new_prev = position
            .checked_sub(1)
            .and_then(|p| children.get(p).cloned());
        let new_next = position
            .checked_add(1)
            .and_then(|p| children.get(p).cloned());
        child.adopt_siblings(mc, new_prev, new_next);
    }

    /// Append a child element into the end of the child list of an element node.
    pub fn append_child(&mut self, mc: MutationContext<'gc, '_>, child: XmlNode<'gc>) {
        self.insert_child(mc, self.children_len(), child);
    }

    /// Remove this node from its parent.
    pub fn remove_node(&mut self, mc: MutationContext<'gc, '_>) {
        if let Some(parent) = self.parent() {
            // This is guaranteed to succeed, as `self` is a child of `parent`.
            let position = parent.child_position(*self).unwrap();
            parent.0.write(mc).children.remove(position);

            self.disown_siblings(mc);
            self.disown_parent(mc);
        }
    }

    /// Returns the type of this node as an integer.
    pub fn node_type(self) -> u8 {
        self.0.read().node_type
    }

    /// Returns the tag name of this element, if any.
    pub fn node_name(self) -> Option<AvmString<'gc>> {
        if self.0.read().node_type == ELEMENT_NODE {
            self.0.read().node_value
        } else {
            None
        }
    }

    pub fn local_name(self, gc_context: MutationContext<'gc, '_>) -> Option<AvmString<'gc>> {
        self.node_name().map(|name| match name.find(b':') {
            Some(i) if i + 1 < name.len() => AvmString::new(gc_context, &name[i + 1..]),
            _ => name,
        })
    }

    pub fn prefix(self, gc_context: MutationContext<'gc, '_>) -> Option<AvmString<'gc>> {
        self.node_name().map(|name| match name.find(b':') {
            Some(i) if i + 1 < name.len() => AvmString::new(gc_context, &name[..i]),
            _ => "".into(),
        })
    }

    /// Returns the node value of this node, if any.
    pub fn node_value(self) -> Option<AvmString<'gc>> {
        if self.0.read().node_type == ELEMENT_NODE {
            None
        } else {
            self.0.read().node_value
        }
    }

    pub fn set_node_value(self, gc_context: MutationContext<'gc, '_>, value: AvmString<'gc>) {
        self.0.write(gc_context).node_value = Some(value);
    }

    /// Returns the number of children of the current tree node.
    pub fn children_len(self) -> usize {
        self.0.read().children.len()
    }

    /// Get the position of a child of this node.
    ///
    /// This function yields None if the node cannot accept children or if the
    /// child node is not a child of this node.
    pub fn child_position(self, child: XmlNode<'gc>) -> Option<usize> {
        self.children()
            .position(|other| GcCell::ptr_eq(child.0, other.0))
    }

    /// Checks if `child` is a direct descendant of `self`.
    pub fn has_child(self, child: XmlNode<'gc>) -> bool {
        child
            .parent()
            .filter(|p| GcCell::ptr_eq(self.0, p.0))
            .is_some()
    }

    /// Retrieve a given child by index.
    pub fn get_child_by_index(self, index: usize) -> Option<XmlNode<'gc>> {
        self.0.read().children.get(index).cloned()
    }

    /// Returns an iterator that yields child nodes.
    pub fn children(self) -> impl DoubleEndedIterator<Item = XmlNode<'gc>> {
        xml::iterators::ChildIter::for_node(self)
    }

    /// Returns an iterator that yields ancestor nodes (including itself).
    pub fn ancestors(self) -> impl Iterator<Item = XmlNode<'gc>> {
        xml::iterators::AnscIter::for_node(self)
    }

    /// Get the already-instantiated script object from the current node.
    fn get_script_object(self) -> Option<Object<'gc>> {
        self.0.read().script_object
    }

    /// Introduce this node to a new script object.
    ///
    /// This internal function *will* overwrite already extant objects, so only
    /// call this if you need to instantiate the script object for the first
    /// time. Attempting to call it a second time will panic.
    pub fn introduce_script_object(
        &mut self,
        gc_context: MutationContext<'gc, '_>,
        new_object: Object<'gc>,
    ) {
        assert!(self.get_script_object().is_none(), "An attempt was made to change the already-established link between script object and XML node. This has been denied and is likely a bug.");
        self.0.write(gc_context).script_object = Some(new_object);
    }

    /// Obtain the script object for a given XML tree node, constructing a new
    /// script object if one does not exist.
    pub fn script_object(&mut self, activation: &mut Activation<'_, 'gc, '_>) -> Object<'gc> {
        match self.get_script_object() {
            Some(object) => object,
            None => {
                let proto = activation.context.avm1.prototypes().xml_node;
                XmlNodeObject::from_xml_node(activation.context.gc_context, *self, Some(proto))
                    .into()
            }
        }
    }

    /// Obtain the script object for a given XML tree node's attributes,
    /// constructing a new script object if one does not exist.
    pub fn attribute_script_object(&mut self, gc_context: MutationContext<'gc, '_>) -> Object<'gc> {
        *self
            .0
            .write(gc_context)
            .attributes_script_object
            .get_or_insert_with(|| XmlAttributesObject::from_xml_node(gc_context, *self))
    }

    /// Swap the contents of this node with another one.
    ///
    /// After this function completes, the current `XMLNode` will contain all
    /// data present in the `other` node, and vice versa. References to the node
    /// will *not* be updated: it is a logic error to swap nodes that have
    /// existing referents.
    pub fn swap(&mut self, gc_context: MutationContext<'gc, '_>, other: Self) {
        if !GcCell::ptr_eq(self.0, other.0) {
            swap(
                &mut *self.0.write(gc_context),
                &mut *other.0.write(gc_context),
            );
        }
    }

    /// Create a duplicate copy of this node.
    ///
    /// If the `deep` flag is set true, then the entire node tree will be cloned.
    pub fn duplicate(self, gc_context: MutationContext<'gc, '_>, deep: bool) -> XmlNode<'gc> {
        let mut clone = XmlNode(GcCell::allocate(
            gc_context,
            XmlNodeData {
                script_object: None,
                attributes_script_object: None,
                parent: None,
                prev_sibling: None,
                next_sibling: None,
                node_type: self.0.read().node_type,
                node_value: self.0.read().node_value,
                attributes: self.0.read().attributes.clone(),
                children: Vec::new(),
            },
        ));

        if deep {
            for (position, child) in self.children().enumerate() {
                clone.insert_child(gc_context, position, child.duplicate(gc_context, deep));
            }
        }

        clone
    }

    /// Retrieve the value of a single attribute on this node.
    pub fn attribute_value(self, name: &WStr) -> Option<AvmString<'gc>> {
        self.0.read().attributes.get(name).copied()
    }

    /// Retrieve all keys defined on this node.
    pub fn attribute_keys(self) -> Vec<AvmString<'gc>> {
        self.0.read().attributes.keys().cloned().collect()
    }

    /// Set the value of a single attribute on this node.
    pub fn set_attribute_value(
        self,
        gc_context: MutationContext<'gc, '_>,
        name: AvmString<'gc>,
        value: AvmString<'gc>,
    ) {
        self.0.write(gc_context).attributes.insert(name, value);
    }

    /// Delete the value of a single attribute on this node.
    pub fn delete_attribute(self, gc_context: MutationContext<'gc, '_>, name: &WStr) {
        self.0.write(gc_context).attributes.remove(name);
    }

    /// Look up the URI for the given namespace.
    ///
    /// XML namespaces are determined by `xmlns:` namespace attributes on the
    /// current node, or its parent.
    pub fn lookup_uri_for_namespace(self, namespace: &WStr) -> Option<AvmString<'gc>> {
        let mut attribute_name = WString::from_buf(b"xmlns:".to_vec());
        attribute_name.push_str(namespace);

        for node in self.ancestors() {
            if namespace.is_empty() {
                if let Some(url) = node.attribute_value(WStr::from_units(b"xmlns")) {
                    return Some(url);
                }
            }

            if let Some(url) = node.attribute_value(&attribute_name) {
                return Some(url);
            }
        }

        None
    }

    /// Look up the namespace for the given URI.
    ///
    /// XML namespaces are determined by `xmlns:` namespace attributes on the
    /// current node, or its parent.
    ///
    /// If there are multiple namespaces that match the URI, the first
    /// mentioned on the closest node will be returned.
    pub fn lookup_namespace_for_uri(self, uri: &WStr) -> Option<WString> {
        for node in self.ancestors() {
            for (attr, attr_value) in node.0.read().attributes.iter() {
                if attr_value == uri {
                    if attr == b"xmlns" {
                        return Some(WString::new());
                    } else if attr.starts_with(WStr::from_units(b"xmlns:")) {
                        return Some(attr[b"xmlns:".len()..].into());
                    }
                }
            }
        }

        None
    }

    /// Convert the given node to a string of UTF-8 encoded XML.
    pub fn into_string(self) -> WString {
        let mut result = WString::new();
        self.write_node_to_string(&mut result);
        result
    }

    /// Write the contents of this node, including its children, to the given string.
    fn write_node_to_string(self, result: &mut WString) {
        // TODO: we convert some strings to utf8, replacing unpaired surrogates by the replacement char.
        // It is correct?
        if self.0.read().node_type == ELEMENT_NODE {
            let children = &self.0.read().children;
            if let Some(tag_name) = self.0.read().node_value {
                result.push_byte(b'<');
                result.push_str(&tag_name);

                for (key, value) in &self.0.read().attributes {
                    result.push_byte(b' ');
                    result.push_str(&key);
                    result.push_str(WStr::from_units(b"=\""));
                    let encoded_value = value.to_utf8_lossy();
                    let escaped_value = escape(encoded_value.as_bytes());
                    result.push_str(WStr::from_units(&*escaped_value));
                    result.push_byte(b'"');
                }

                if children.is_empty() {
                    result.push_str(WStr::from_units(b" />"));
                } else {
                    result.push_byte(b'>');
                    for child in children {
                        child.write_node_to_string(result);
                    }
                    result.push_str(WStr::from_units(b"</"));
                    result.push_str(&tag_name);
                    result.push_byte(b'>');
                }
            } else {
                for child in children {
                    child.write_node_to_string(result);
                }
            }
        } else {
            let value = self.0.read().node_value.unwrap();
            let encoded = value.to_utf8_lossy();
            let escaped = escape(encoded.as_bytes());
            result.push_str(WStr::from_units(&*escaped));
        }
    }
}

impl<'gc> fmt::Debug for XmlNode<'gc> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("XmlNodeData")
            .field("0", &self.0.as_ptr())
            .field(
                "script_object",
                &self
                    .0
                    .read()
                    .script_object
                    .map(|p| format!("{:p}", p.as_ptr()))
                    .unwrap_or_else(|| "None".to_string()),
            )
            .field(
                "parent",
                &self
                    .0
                    .read()
                    .parent
                    .map(|p| format!("{:p}", p.0.as_ptr()))
                    .unwrap_or_else(|| "None".to_string()),
            )
            .field("node_value", &self.0.read().node_value)
            .field("attributes", &self.0.read().attributes)
            .field("children", &self.0.read().children)
            .finish()
    }
}
