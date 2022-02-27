mod lang;

use std::io::Error;
use lang::code_analysis::syntax::lexer::Lexer;
use crate::lang::code_analysis::syntax::parser::Parser;
use crate::lang::code_generator::wasm_generator::WasmGenerator;
use crate::lang::semantic_analysis::analyzer::Anaylzer;
fn main() ->Result<(),Error>
{

    let input_text=r#"
    fun get(a:int,b:float)
    {
        //comment
        let d=-a;
        let f=b+1.0;
        /* some multi line hints */
        return;
    }
   "#;

    let lexer= Lexer::new(input_text.to_string());
    let mut parser=Parser::new(lexer);
    let ast=parser.parse()?;
    let mut analyzer=Anaylzer::new(&ast);
    let symbol_info=analyzer.analyze()?;
    let mut generator=WasmGenerator::new(&ast,&symbol_info);
    let text=generator.build()?;
    println!("{}",text.to_string());
    println!("generated assembly successfully");
    Ok(())
}

