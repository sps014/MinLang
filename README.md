# Syntax Expression Tree
It is a Recursive Descent Parser. It is a kind of Top-Down Parser. A top-down parser builds the parse tree from the top to down, starting with the start non-terminal. A Predictive Parser is a special case of Recursive Descent Parser, where no Back Tracking is required.

<It provide  Base or foundation for writing compilers, Left Recursion Free CFG can be handled by this and custom facilities can be added by Extending `Lexer` , `Parser` and `Bound Tree`. 

<br/>Language can parse , evaluate and generate syntax tree for conditionals and iterations statements with endless nesting
eg.

```py
a=7

if a==7
{
  print(a)
}

while(a)
{
  print(a)
  a++
  if(a>20)
   a=0
}

#and so on
```


### Steps to Build
1. Install Rust Language Compiler, if not already installed.
2. Clone the Repo.
3. Go to repo and run following command in terminal
```
cargo run
```
