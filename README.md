# MinLang
A minimal statically typed turing complete scripting language.

#### features
1. Statically typed
2. Type inferencing
3. turing complete
4. Semantic analyzer with Control FLow analysis
5. ultra fast
6. Clear error messages
7. web assembly text code generator 


eg. 

```kt
    fun get_pi() :float
    {
        return 3.14;
    }
    fun abc(test:int,alpha:float):float
    {
        let b=get_pi(5); // infered to float
        let d="this is \"some\" string"+"another";
        let a=0.0; // infered as float
        let count=0; // infered as int
        while a<b // no parenthesis required in condition
        {
           a=a+1;
           //Infinite nesting
           while 1
           {
              count=count+1;
              if count>100 //supports if else ladder
              {
                 break; // support break and continue
              }
           }
        }
        return a; //return control flow analysis
    }

//and so on
```


### Steps to Build
1. Install Rust Language Compiler, if not already installed.
2. Clone the Repo.
3. Go to repo and run following command in terminal
```
cargo run
```
