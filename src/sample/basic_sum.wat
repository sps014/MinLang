(module
	(func $factorial( param $n i32)  (result i32) (local $r i32)  (local $i i32)  (local $n i32) 
		i32.const 1
		local.set $r
		i32.const 2
		local.set $i
		(block
			(loop
				local.get $i
				local.get $n
				i32.le_s
				i32.const 0
				i32.eq
				br_if 1
				local.get $r
				local.get $i
				i32.mul
				local.set $r
				local.get $i
				i32.const 1
				i32.add
				local.set $i
				br 0
			)
		)
		local.get $r
		return
	)
	(export "factorial" (func $factorial))
)
