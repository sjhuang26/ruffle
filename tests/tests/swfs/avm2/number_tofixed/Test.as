﻿package {
	public class Test {
	}
}

function assert_exp(val) {
	trace("///(digits = ?!)");
	trace(val.toFixed());
	trace("///(digits = 0)");
	trace(val.toFixed(0));
	trace("///(digits = 1)");
	trace(val.toFixed(1));
	trace("///(digits = 2)");
	trace(val.toFixed(2));
	trace("///(digits = 3)");
	trace(val.toFixed(3));
	trace("///(digits = 4)");
	trace(val.toFixed(4));
	trace("///(digits = 5)");
	trace(val.toFixed(5));
	trace("///(digits = 6)");
	trace(val.toFixed(6));
	trace("///(digits = 7)");
	trace(val.toFixed(7));
	trace("///(digits = 8)");
	trace(val.toFixed(8));
	trace("///(digits = 9)");
	trace(val.toFixed(9));
	trace("///(digits = 10)");
	trace(val.toFixed(10));
	trace("///(digits = 20)");
	trace(val.toFixed(20));
}

trace("//1.2315e-8");
assert_exp(1.2315e-8);

trace("//1.2315e-7");
assert_exp(1.2315e-7);

trace("//1.2315e-6");
assert_exp(1.2315e-6);

trace("//1.2315e2");
assert_exp(1.2315e2);

trace("//1.2315e19");
assert_exp(1.2315e19);

trace("//1.2315e20");
assert_exp(1.2315e20);

trace("//1.2315e21");
assert_exp(1.2315e21);

trace("//1.2315987654321987654321987654321987654321987654321987654321e-8");
assert_exp(1.2315987654321987654321987654321987654321987654321987654321e-8);

trace("//1.2315987654321987654321987654321987654321987654321987654321e-7");
assert_exp(1.2315987654321987654321987654321987654321987654321987654321e-7);

trace("//1.2315987654321987654321987654321987654321987654321987654321e-6");
assert_exp(1.2315987654321987654321987654321987654321987654321987654321e-6);

trace("//1.2315987654321987654321987654321987654321987654321987654321e2");
assert_exp(1.2315987654321987654321987654321987654321987654321987654321e2);

trace("//1.2315987654321987654321987654321987654321987654321987654321e19");
assert_exp(1.2315987654321987654321987654321987654321987654321987654321e19);

trace("//1.2315987654321987654321987654321987654321987654321987654321e20");
assert_exp(1.2315987654321987654321987654321987654321987654321987654321e20);

trace("//1.2315987654321987654321987654321987654321987654321987654321e21");
assert_exp(1.2315987654321987654321987654321987654321987654321987654321e21);