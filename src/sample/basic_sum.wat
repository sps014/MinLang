(module
	(func $sum( param $a i32) ( param $b i32)  (result i32) (local $b i32)  (local $a i32)  (local $i i32) 
		i32.const 0
		local.set $i
		(block
			(loop
				i32.const 1
				i32.const 0
				i32.eq
				br_if 1
				local.get $i
				i32.const 1
				i32.add
				local.set $i
				local.get $i
				i32.const 5
				i32.ge_s
				(if
					(then
						br 1
					)
				)
				br 0
			)
		)
		local.get $a
		local.get $b
		i32.add
		return
	)
	(export "sum" (func $sum))
)
