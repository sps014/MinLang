(func $strlen (param $ptr i32) (result i32)
    (local $len i32)
    i32.const 0
    local.set $len
    (block $end
        (loop $start
            local.get $ptr
            local.get $len
            i32.add
            i32.load8_u
            i32.eqz
            br_if $end
            local.get $len
            i32.const 1
            i32.add
            local.set $len
            br $start
        )
    )
    local.get $len
)

(func $concat_strings (param $str1 i32) (param $str2 i32) (result i32)
    (local $len1 i32)
    (local $len2 i32)
    (local $new_ptr i32)
    (local $i i32)
    local.get $str1
    call $strlen
    local.set $len1
    local.get $str2
    call $strlen
    local.set $len2
    local.get $len1
    local.get $len2
    i32.add
    i32.const 1
    i32.add
    i32.const 5
    call $malloc
    local.set $new_ptr
    i32.const 0
    local.set $i
    (block $end1
        (loop $start1
            local.get $i
            local.get $len1
            i32.eq
            br_if $end1
            local.get $new_ptr
            local.get $i
            i32.add
            local.get $str1
            local.get $i
            i32.add
            i32.load8_u
            i32.store8
            local.get $i
            i32.const 1
            i32.add
            local.set $i
            br $start1
        )
    )
    i32.const 0
    local.set $i
    (block $end2
        (loop $start2
            local.get $i
            local.get $len2
            i32.eq
            br_if $end2
            local.get $new_ptr
            local.get $len1
            i32.add
            local.get $i
            i32.add
            local.get $str2
            local.get $i
            i32.add
            i32.load8_u
            i32.store8
            local.get $i
            i32.const 1
            i32.add
            local.set $i
            br $start2
        )
    )
    local.get $new_ptr
    local.get $len1
    local.get $len2
    i32.add
    i32.add
    i32.const 0
    i32.store8
    local.get $new_ptr
)

(func $debug_get_free_list_head (result i32)
    global.get $free_list_head
)

(func $string_eq (param $a i32) (param $b i32) (result i32)
    (local $ca i32)
    (local $cb i32)
    ;; identical pointers (covers the both-null case) are trivially equal
    local.get $a
    local.get $b
    i32.eq
    if
        i32.const 1
        return
    end
    ;; a null pointer can only equal another null pointer (handled above)
    local.get $a
    i32.eqz
    if
        i32.const 0
        return
    end
    local.get $b
    i32.eqz
    if
        i32.const 0
        return
    end
    (block $done
        (loop $cmp
            local.get $a
            i32.load8_u
            local.set $ca
            local.get $b
            i32.load8_u
            local.set $cb
            local.get $ca
            local.get $cb
            i32.ne
            if
                i32.const 0
                return
            end
            local.get $ca
            i32.eqz
            if
                i32.const 1
                return
            end
            local.get $a
            i32.const 1
            i32.add
            local.set $a
            local.get $b
            i32.const 1
            i32.add
            local.set $b
            br $cmp
        )
    )
    i32.const 0
)

(func $char_at (param $ptr i32) (param $i i32) (result i32)
    local.get $ptr
    local.get $i
    i32.add
    i32.load8_u
)

(func $string_alloc (param $n i32) (result i32)
    (local $p i32)
    ;; n data bytes + 1 null terminator
    local.get $n
    i32.const 1
    i32.add
    i32.const 5
    call $malloc
    local.set $p
    ;; write the null terminator at [n]; the n data bytes are filled by the caller via $string_set
    local.get $p
    local.get $n
    i32.add
    i32.const 0
    i32.store8
    local.get $p
)

(func $string_set (param $ptr i32) (param $i i32) (param $c i32)
    local.get $ptr
    local.get $i
    i32.add
    local.get $c
    i32.store8
)
