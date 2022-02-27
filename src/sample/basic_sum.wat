(module
	(func $sum( param $a i32) ( param $b i32)  (result i32) (local $b i32)  (local $a i32) 
		local.get $a
		local.get $b
		i32.add
		return
	)
	(export "sum" (func $sum))
)
