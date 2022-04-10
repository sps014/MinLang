fun factorial(n:int):int
{
    let r=1;
    let i=2;
    while i<=n
    {
        r=r*i;
        i=i+1;
    }
    return r;
}