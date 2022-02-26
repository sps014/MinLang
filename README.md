# MinLang
A minimal statically typed turing complete scripting language.

#### features
1. Statically typed
2. Type inferencing
3. turing complete
4. Semantic analyzer
5. ultra fast
5. Clear error messages
5. web assembly text code generator (wip)
6. standard library (todo)


eg. 

```kt
    fun get_pi() :float
    {
        return 3.14;
    }
    fun abc(test:int,alpha:float):float
    {
        let b=get_pi(5);
        let d="this is \"some\" string"+"another";
        let a=0.0;
        while a<b
        {
           a=a+1;
        }
        return a;
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
