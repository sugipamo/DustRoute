use crate::logic::{DagBuilder, GateKind, LogicDag};

pub fn half_adder() -> LogicDag {
    let mut b = DagBuilder::new();
    let a = b.input("a");
    let c = b.input("b");
    let sum = b.gate(GateKind::Xor, &[a, c], Some("sum_xor"));
    let carry = b.gate(GateKind::And, &[a, c], Some("carry_and"));
    b.finish([("sum".into(), sum), ("carry".into(), carry)])
        .expect("built-in circuit is valid")
}

pub fn half_subtractor() -> LogicDag {
    let mut b = DagBuilder::new();
    let a = b.input("a");
    let c = b.input("b");
    let difference = b.gate(GateKind::Xor, &[a, c], Some("difference_xor"));
    let not_a = b.gate(GateKind::Not, &[a], Some("not_a"));
    let borrow = b.gate(GateKind::And, &[not_a, c], Some("borrow_and"));
    b.finish([("difference".into(), difference), ("borrow".into(), borrow)])
        .expect("built-in circuit is valid")
}

pub fn mux_2_to_1() -> LogicDag {
    let mut b = DagBuilder::new();
    let a = b.input("a");
    let c = b.input("b");
    let select = b.input("s");
    let not_select = b.gate(GateKind::Not, &[select], Some("not_s"));
    let left = b.gate(GateKind::And, &[a, not_select], Some("select_a"));
    let right = b.gate(GateKind::And, &[c, select], Some("select_b"));
    let output = b.gate(GateKind::Or, &[left, right], Some("mux_out"));
    b.finish([("out".into(), output)])
        .expect("built-in circuit is valid")
}

pub fn decoder_1_to_2() -> LogicDag {
    let mut b = DagBuilder::new();
    let input = b.input("input");
    let enable = b.input("enable");
    let not_input = b.gate(GateKind::Not, &[input], Some("not_input"));
    let out_0 = b.gate(GateKind::And, &[enable, not_input], Some("out_0"));
    let out_1 = b.gate(GateKind::And, &[enable, input], Some("out_1"));
    b.finish([("out0".into(), out_0), ("out1".into(), out_1)])
        .expect("built-in circuit is valid")
}

pub fn full_adder() -> LogicDag {
    let mut b = DagBuilder::new();
    let a = b.input("a");
    let c = b.input("b");
    let carry_in = b.input("carry_in");
    let a_xor_b = b.gate(GateKind::Xor, &[a, c], Some("a_xor_b"));
    let sum = b.gate(GateKind::Xor, &[a_xor_b, carry_in], Some("sum"));
    let carry_ab = b.gate(GateKind::And, &[a, c], Some("carry_ab"));
    let carry_xor = b.gate(GateKind::And, &[a_xor_b, carry_in], Some("carry_xor"));
    let carry = b.gate(GateKind::Or, &[carry_ab, carry_xor], Some("carry"));
    b.finish([("sum".into(), sum), ("carry".into(), carry)])
        .expect("built-in circuit is valid")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn inputs(values: &[(&str, bool)]) -> HashMap<String, bool> {
        values
            .iter()
            .map(|(name, value)| ((*name).into(), *value))
            .collect()
    }

    #[test]
    fn half_adder_truth_table_and_lowering() {
        for a in [false, true] {
            for b in [false, true] {
                let values = inputs(&[("a", a), ("b", b)]);
                let expected = half_adder().evaluate(&values).unwrap();
                assert_eq!(expected["sum"], a ^ b);
                assert_eq!(expected["carry"], a && b);
                assert_eq!(
                    half_adder().lower_xor().unwrap().evaluate(&values).unwrap(),
                    expected
                );
            }
        }
    }

    #[test]
    fn mux_truth_table() {
        for a in [false, true] {
            for b in [false, true] {
                for s in [false, true] {
                    let result = mux_2_to_1()
                        .evaluate(&inputs(&[("a", a), ("b", b), ("s", s)]))
                        .unwrap();
                    assert_eq!(result["out"], if s { b } else { a });
                }
            }
        }
    }

    #[test]
    fn full_adder_truth_table() {
        for a in [false, true] {
            for b in [false, true] {
                for carry_in in [false, true] {
                    let result = full_adder()
                        .evaluate(&inputs(&[("a", a), ("b", b), ("carry_in", carry_in)]))
                        .unwrap();
                    let count = u8::from(a) + u8::from(b) + u8::from(carry_in);
                    assert_eq!(result["sum"], count % 2 == 1);
                    assert_eq!(result["carry"], count >= 2);
                }
            }
        }
    }
}
