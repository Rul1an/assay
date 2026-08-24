use serde_jcs::to_string;
use serde_json::Error;
use serde_json::Value;
use serde_json::from_str;

#[track_caller]
fn assert_json(input: &str, expected: &str) {
  let json_val: Value = from_str(input).unwrap();
  let received: String = to_string(&json_val).unwrap();

  assert_eq!(received.trim(), expected.trim());
}

#[track_caller]
fn assert_json_number(input: u64, expected: &str) {
  let json_val: f64 = f64::from_bits(input);
  let received: String = to_string(&json_val).unwrap();

  assert_eq!(received, expected);
}

#[track_caller]
fn assert_json_number_err(input: u64) {
  let json_val: f64 = f64::from_bits(input);
  let received: Result<String, Error> = to_string(&json_val);

  assert!(received.is_err());
}

#[test]
fn test_rfc_appendix_b() {
  assert_json_number(0x0000_0000_0000_0000, "0"); // Zero
  assert_json_number(0x8000_0000_0000_0000, "0"); // Minus zero
  assert_json_number(0x0000_0000_0000_0001, "5e-324"); // Min pos number
  assert_json_number(0x8000_0000_0000_0001, "-5e-324"); // Min neg number
  assert_json_number(0x7fef_ffff_ffff_ffff, "1.7976931348623157e+308"); // Max pos number
  assert_json_number(0xffef_ffff_ffff_ffff, "-1.7976931348623157e+308"); // Max neg number
  assert_json_number(0x4340_0000_0000_0000, "9007199254740992"); // Max pos int
  assert_json_number(0xc340_0000_0000_0000, "-9007199254740992"); // Max neg int
  assert_json_number(0x4430_0000_0000_0000, "295147905179352830000"); // ~2**68

  assert_json_number_err(0x7fff_ffff_ffff_ffff); // NaN
  assert_json_number_err(0x7ff0_0000_0000_0000); // Infinity

  assert_json_number(0x44b5_2d02_c7e1_4af5, "9.999999999999997e+22");
  assert_json_number(0x44b5_2d02_c7e1_4af6, "1e+23");
  assert_json_number(0x44b5_2d02_c7e1_4af7, "1.0000000000000001e+23");
  assert_json_number(0x444b_1ae4_d6e2_ef4e, "999999999999999700000");
  assert_json_number(0x444b_1ae4_d6e2_ef4f, "999999999999999900000");
  assert_json_number(0x444b_1ae4_d6e2_ef50, "1e+21");
  assert_json_number(0x3eb0_c6f7_a0b5_ed8c, "9.999999999999997e-7");
  assert_json_number(0x3eb0_c6f7_a0b5_ed8d, "0.000001");
  assert_json_number(0x41b3_de43_5555_5553, "333333333.3333332");
  assert_json_number(0x41b3_de43_5555_5554, "333333333.33333325");
  assert_json_number(0x41b3_de43_5555_5555, "333333333.3333333");
  assert_json_number(0x41b3_de43_5555_5556, "333333333.3333334");
  assert_json_number(0x41b3_de43_5555_5557, "333333333.33333343");
  assert_json_number(0xbecb_f647_612f_3696, "-0.0000033333333333333333");
  assert_json_number(0x4314_3ff3_c1cb_0959, "1424953923781206.2"); // Round to even
}

#[test]
fn test_rfc_appendix_c() {
  assert_json(
    include_str!("fixtures/input/appendix_c.json"),
    include_str!("fixtures/output/appendix_c.json"),
  );
}

#[test]
fn test_rfc_appendix_e() {
  assert_json(
    include_str!("fixtures/input/appendix_e.json"),
    include_str!("fixtures/output/appendix_e.json"),
  );
}

#[test]
fn test_rfc_appendix_i_arrays() {
  assert_json(
    include_str!("testdata/input/arrays.json"),
    include_str!("testdata/output/arrays.json"),
  );
}

#[test]
fn test_rfc_appendix_i_french() {
  assert_json(
    include_str!("testdata/input/french.json"),
    include_str!("testdata/output/french.json"),
  );
}

#[test]
fn test_rfc_appendix_i_structures() {
  assert_json(
    include_str!("testdata/input/structures.json"),
    include_str!("testdata/output/structures.json"),
  );
}

#[test]
fn test_rfc_appendix_i_unicode() {
  assert_json(
    include_str!("testdata/input/unicode.json"),
    include_str!("testdata/output/unicode.json"),
  );
}

#[test]
fn test_rfc_appendix_i_values() {
  assert_json(
    include_str!("testdata/input/values.json"),
    include_str!("testdata/output/values.json"),
  );
}

#[test]
fn test_rfc_appendix_i_weird() {
  assert_json(
    include_str!("testdata/input/weird.json"),
    include_str!("testdata/output/weird.json"),
  );
}
