mod lang;
use lang::code_analysis::syntax::lexer::Lexer;
use crate::lang::code_analysis::syntax::parser::Parser;
use crate::lang::semantic_analysis::analyzer::Anaylzer;
fn main() {

    let input_text=r#"
    fun get_pi(a:int) :float
    {
        if 1==2
        {
          return 3.14;
        }
        else if 1==2
        {
          return 6.0;
        }
        else
        {
           if 1<5
           {
             return 3.14;
           }
           else
           {

           }
        }

    }
   "#;

    let lexer= Lexer::new(input_text.to_string());
    let parser=Parser::new(lexer);
    let mut analyzer=Anaylzer::new(parser);
    let result=analyzer.analyze();
    match result{
        Ok(()) =>
            println!("No errors found"),

        Err(e) => println!("error: {}",e),
    }
}

