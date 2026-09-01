use pest::Parser as _;
use dhv::parser::HslParser;
use dhv::parser::Rule;

#[test]
fn raw_string_probe() {
    for (label, rule, input) in [
        ("string_lit", Rule::string_literal, "\"x\""),
        ("raw_string r\"x\"", Rule::raw_string_lit, "r\"x\""),
        ("raw hash1", Rule::raw_string_lit, "r#\"a\"#"),
        ("raw r\"abc\"", Rule::raw_string_lit, "r\"abc\""),
    ] {
        let r = HslParser::parse(rule, input);
        match r {
            Ok(p) => println!("{label}: OK consumed {:?}", p.clone().last().map(|q| q.as_str().to_string())),
            Err(e) => println!("{label}: ERR {e:?}"),
        }
    }
    // let_statement 完整
    let r = HslParser::parse(Rule::let_statement, "let s = r\"x\"; ");
    match r {
        Ok(_p) => println!("let_statement: OK"),
        Err(e) => println!("let_statement: ERR {e}"),
    }
    // expression 完整
    let r = HslParser::parse(Rule::expression, "r\"x\"; ");
    match r {
        Ok(_) => println!("expression r\"x\"; : OK"),
        Err(e) => println!("expression r\"x\"; : ERR {e}"),
    }
}
