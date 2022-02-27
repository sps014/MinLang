mod lang;

use std::io::Error;
use lang::code_analysis::syntax::lexer::Lexer;
use crate::lang::code_analysis::syntax::parser::Parser;
use crate::lang::code_generator::wasm_generator::WasmGenerator;
use crate::lang::semantic_analysis::analyzer::Anaylzer;
fn main() ->Result<(),Error>
{

    let input_text=r#"
    fun sum(a:int,b:int):int
    {
        return a+b;
    }
    fun get(a:int,b:float)
    {
        //comment
        let c=1+sum(a,2);
        let f=b+1.0;
        while a>5
        {
            a=a-1;
            if a>10
            {
                break;
            }
            else if a==1
            {
                continue;
            }
        }
        /* some multi line comments */
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

