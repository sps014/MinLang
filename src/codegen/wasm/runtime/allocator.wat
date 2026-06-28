(func $malloc (param $size i32) (param $tag i32) (result i32)
    (local $curr i32)
    (local $prev i32)
    (local $next i32)
    (local $block_size i32)
    (local $new_ptr i32)
    ;; round size up to a multiple of 4, then reserve 12 bytes for the header
    local.get $size
    i32.const 3
    i32.add
    i32.const -4
    i32.and
    local.set $size
    local.get $size
    i32.const 12
    i32.add
    local.set $size
    ;; scan the freelist for a large-enough block
    global.get $free_list_head
    local.set $curr
    i32.const 0
    local.set $prev
    (block $alloc_done
        (loop $scan_freelist
            local.get $curr
            i32.eqz
            br_if $alloc_done
            local.get $curr
            i32.load
            local.set $block_size
            local.get $block_size
            local.get $size
            i32.ge_s
            (if
                (then
                    ;; unlink this block from the freelist
                    local.get $curr
                    i32.const 4
                    i32.add
                    i32.load
                    local.set $next
                    local.get $prev
                    i32.eqz
                    (if
                        (then
                            local.get $next
                            global.set $free_list_head
                        )
                        (else
                            local.get $prev
                            i32.const 4
                            i32.add
                            local.get $next
                            i32.store
                        )
                    )
                    ;; tag at block+4
                    local.get $curr
                    i32.const 4
                    i32.add
                    local.get $tag
                    i32.store
                    ;; ref_count = 1 at block+8
                    local.get $curr
                    i32.const 8
                    i32.add
                    i32.const 1
                    i32.store
                    ;; return data pointer (block + 12)
                    local.get $curr
                    i32.const 12
                    i32.add
                    return
                )
            )
            local.get $curr
            local.set $prev
            local.get $curr
            i32.const 4
            i32.add
            i32.load
            local.set $curr
            br $scan_freelist
        )
    )
    ;; no free block fit: bump-allocate fresh memory
    global.get $heap_ptr
    local.set $new_ptr
    global.get $heap_ptr
    local.get $size
    i32.add
    global.set $heap_ptr
    local.get $new_ptr
    local.get $size
    i32.store
    local.get $new_ptr
    i32.const 4
    i32.add
    local.get $tag
    i32.store
    local.get $new_ptr
    i32.const 8
    i32.add
    i32.const 1
    i32.store
    local.get $new_ptr
    i32.const 12
    i32.add
)

(func $free (param $ptr i32)
    (local $block_start i32)
    local.get $ptr
    i32.eqz
    br_if 0
    local.get $ptr
    i32.const 12
    i32.sub
    local.set $block_start
    local.get $block_start
    i32.const 4
    i32.add
    global.get $free_list_head
    i32.store
    local.get $block_start
    global.set $free_list_head
)

(func $retain (param $ptr i32)
    (local $ref_count_ptr i32)
    local.get $ptr
    i32.eqz
    br_if 0
    local.get $ptr
    i32.const 4
    i32.sub
    local.set $ref_count_ptr
    local.get $ref_count_ptr
    local.get $ref_count_ptr
    i32.load
    i32.const 1
    i32.add
    i32.store
)

(func $object_tag (param $ptr i32) (result i32)
    local.get $ptr
    i32.eqz
    (if (result i32)
        (then i32.const 0)
        (else
            local.get $ptr
            i32.const 8
            i32.sub
            i32.load
        )
    )
)

(func $release_generic (param $ptr i32)
    (local $ref_count_ptr i32)
    (local $new_count i32)
    local.get $ptr
    i32.eqz
    br_if 0
    local.get $ptr
    i32.const 4
    i32.sub
    local.set $ref_count_ptr
    local.get $ref_count_ptr
    i32.load
    i32.const 1
    i32.sub
    local.set $new_count
    local.get $ref_count_ptr
    local.get $new_count
    i32.store
    local.get $new_count
    i32.eqz
    (if (then
        local.get $ptr
        call $free
    ))
)
