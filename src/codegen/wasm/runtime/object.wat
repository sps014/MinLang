(func $box_int (param $v i32) (result i32)
    (local $p i32)
    i32.const 4
    i32.const 1
    call $malloc
    local.set $p
    local.get $p
    local.get $v
    i32.store
    local.get $p
)
(func $box_float (param $v f32) (result i32)
    (local $p i32)
    i32.const 4
    i32.const 2
    call $malloc
    local.set $p
    local.get $p
    local.get $v
    f32.store
    local.get $p
)
(func $box_double (param $v f64) (result i32)
    (local $p i32)
    i32.const 8
    i32.const 3
    call $malloc
    local.set $p
    local.get $p
    local.get $v
    f64.store
    local.get $p
)
(func $box_bool (param $v i32) (result i32)
    (local $p i32)
    i32.const 4
    i32.const 4
    call $malloc
    local.set $p
    local.get $p
    local.get $v
    i32.store
    local.get $p
)
(func $unbox_int (param $p i32) (result i32)
    local.get $p
    i32.load
)
(func $unbox_float (param $p i32) (result f32)
    local.get $p
    f32.load
)
(func $unbox_double (param $p i32) (result f64)
    local.get $p
    f64.load
)
(func $unbox_bool (param $p i32) (result i32)
    local.get $p
    i32.load
)
(func $hash_int (param $v i32) (result i32)
    local.get $v
)
(func $hash_bool (param $v i32) (result i32)
    local.get $v
)
(func $hash_float (param $v f32) (result i32)
    local.get $v
    i32.reinterpret_f32
)
(func $hash_double (param $v f64) (result i32)
    local.get $v
    f32.demote_f64
    i32.reinterpret_f32
)
(func $hash_string (param $p i32) (result i32)
    (local $h i32)
    (local $i i32)
    (local $c i32)
    i32.const -2128831035
    local.set $h
    i32.const 0
    local.set $i
    (block $done
        (loop $scan
            local.get $p
            local.get $i
            i32.add
            i32.load8_u
            local.set $c
            local.get $c
            i32.eqz
            br_if $done
            local.get $h
            local.get $c
            i32.xor
            local.set $h
            local.get $h
            i32.const 16777619
            i32.mul
            local.set $h
            local.get $i
            i32.const 1
            i32.add
            local.set $i
            br $scan
        )
    )
    local.get $h
)
(func $int_to_string (param $v i32) (result i32)
    (local $p i32)
    (local $i i32)
    (local $neg i32)
    (local $start i32)
    (local $end i32)
    (local $tmp i32)
    (local $digit i32)
    i32.const 16
    i32.const 5
    call $malloc
    local.set $p
    local.get $v
    i32.eqz
    (if (then
        local.get $p
        i32.const 48
        i32.store8
        local.get $p
        i32.const 1
        i32.add
        i32.const 0
        i32.store8
        local.get $p
        return
    ))
    i32.const 0
    local.set $neg
    local.get $v
    i32.const 0
    i32.lt_s
    (if (then
        i32.const 1
        local.set $neg
        i32.const 0
        local.get $v
        i32.sub
        local.set $v
    ))
    i32.const 0
    local.set $i
    (block $gen_done
        (loop $gen
            local.get $v
            i32.eqz
            br_if $gen_done
            local.get $v
            i32.const 10
            i32.rem_s
            local.set $digit
            local.get $p
            local.get $i
            i32.add
            local.get $digit
            i32.const 48
            i32.add
            i32.store8
            local.get $i
            i32.const 1
            i32.add
            local.set $i
            local.get $v
            i32.const 10
            i32.div_s
            local.set $v
            br $gen
        )
    )
    local.get $neg
    (if (then
        local.get $p
        local.get $i
        i32.add
        i32.const 45
        i32.store8
        local.get $i
        i32.const 1
        i32.add
        local.set $i
    ))
    local.get $p
    local.get $i
    i32.add
    i32.const 0
    i32.store8
    i32.const 0
    local.set $start
    local.get $i
    i32.const 1
    i32.sub
    local.set $end
    (block $rev_done
        (loop $rev
            local.get $start
            local.get $end
            i32.ge_s
            br_if $rev_done
            local.get $p
            local.get $start
            i32.add
            i32.load8_u
            local.set $tmp
            local.get $p
            local.get $start
            i32.add
            local.get $p
            local.get $end
            i32.add
            i32.load8_u
            i32.store8
            local.get $p
            local.get $end
            i32.add
            local.get $tmp
            i32.store8
            local.get $start
            i32.const 1
            i32.add
            local.set $start
            local.get $end
            i32.const 1
            i32.sub
            local.set $end
            br $rev
        )
    )
    local.get $p
)
(func $box_char (param $v i32) (result i32)
    (local $p i32)
    i32.const 4
    i32.const 7
    call $malloc
    local.set $p
    local.get $p
    local.get $v
    i32.store
    local.get $p
)
(func $unbox_char (param $p i32) (result i32)
    local.get $p
    i32.load
)
(func $char_to_string (param $v i32) (result i32)
    (local $p i32)
    i32.const 2
    i32.const 5
    call $malloc
    local.set $p
    local.get $p
    local.get $v
    i32.store8
    local.get $p
    i32.const 1
    i32.add
    i32.const 0
    i32.store8
    local.get $p
)
