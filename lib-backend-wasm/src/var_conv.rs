use wasmgen::Scratch;

pub fn encode_vartype(ir_vartype: ir::VarType) -> &'static [wasmgen::ValType] {
    match ir_vartype {
        ir::VarType::Any => &[wasmgen::ValType::I32, wasmgen::ValType::I64],
        ir::VarType::Unassigned => panic!("ICE: IR->Wasm: Unassigned type may not be encoded"),
        ir::VarType::Undefined => &[],
        ir::VarType::Number => &[wasmgen::ValType::F64],
        ir::VarType::Boolean => &[wasmgen::ValType::I32],
        ir::VarType::String => &[wasmgen::ValType::I32],
        ir::VarType::Func => &[wasmgen::ValType::I32, wasmgen::ValType::I32],
        ir::VarType::StructT { typeidx: _ } => &[wasmgen::ValType::I32],
    }
}

// stores a ir variable from the protected stack to local variable(s)
// net wasm stack: [<ir_source_vartype>] -> []
pub fn encode_store_local(
    wasm_localidx: &[wasmgen::LocalIdx],
    ir_dest_vartype: ir::VarType,
    ir_source_vartype: ir::VarType,
    expr_builder: &mut wasmgen::ExprBuilder,
) {
    if ir_dest_vartype == ir_source_vartype {
        match ir_dest_vartype {
            ir::VarType::Any | ir::VarType::Func => {
                assert!(wasm_localidx.len() == 2);
                expr_builder.local_set(wasm_localidx[0]);
                expr_builder.local_set(wasm_localidx[1]);
            }
            ir::VarType::Number | ir::VarType::Boolean | ir::VarType::String => {
                assert!(wasm_localidx.len() == 1);
                expr_builder.local_set(wasm_localidx[0]);
            }
            ir::VarType::StructT { typeidx: _ } => {
                assert!(wasm_localidx.len() == 1);
                expr_builder.local_set(wasm_localidx[0]);
            }
            ir::VarType::Undefined => {
                assert!(wasm_localidx.len() == 0);
            }
            ir::VarType::Unassigned => {
                panic!("ICE: IR->Wasm: Local static vartype cannot be unassigned");
            }
        }
    } else if ir_dest_vartype == ir::VarType::Any {
        // writing from a specific type to the Any type
        assert!(wasm_localidx.len() == 2);
        match ir_source_vartype {
            ir::VarType::Any => {
                panic!("ICE");
            }
            ir::VarType::Undefined => {
                expr_builder.i32_const(ir_source_vartype.tag());
                expr_builder.local_set(wasm_localidx[0]);
            }
            ir::VarType::Unassigned => {
                panic!("ICE: IR->Wasm: Cannot assign to local from unassigned value");
            }
            ir::VarType::Number => {
                expr_builder.i32_const(ir_source_vartype.tag());
                expr_builder.local_set(wasm_localidx[0]);
                expr_builder.i64_reinterpret_f64(); // convert f64 to i64
                expr_builder.local_set(wasm_localidx[1]);
            }
            ir::VarType::Boolean | ir::VarType::String => {
                expr_builder.i32_const(ir_source_vartype.tag());
                expr_builder.local_set(wasm_localidx[0]);
                expr_builder.i64_extend_i32_u(); // convert i32 to i64
                expr_builder.local_set(wasm_localidx[1]);
            }
            ir::VarType::StructT { typeidx: _ } => {
                expr_builder.i32_const(ir_source_vartype.tag());
                expr_builder.local_set(wasm_localidx[0]);
                expr_builder.i64_extend_i32_u(); // convert i32 to i64
                expr_builder.local_set(wasm_localidx[1]);
            }
            ir::VarType::Func => {
                expr_builder.i32_const(ir_source_vartype.tag());
                expr_builder.local_set(wasm_localidx[0]);
                // the rest of the instructions concats the two i32s from the stack into the i64 local
                expr_builder.i64_extend_i32_u(); // convert i32 to i64 (index in table)
                expr_builder.local_set(wasm_localidx[1]);
                expr_builder.i64_extend_i32_u(); // convert i32 to i64 (ptr to closure)
                expr_builder.i64_const(32);
                expr_builder.i64_shl();
                expr_builder.local_get(wasm_localidx[1]);
                expr_builder.i64_or();
                expr_builder.local_set(wasm_localidx[1]);
            }
        }
    } else {
        panic!("ICE: IR->Wasm: Assignment to local is not equivalent or widening conversion");
    }
}

// stores a ir variable from the protected stack to global variable(s)
// equivalent to encode_store_local() but for globals
// net wasm stack: [<ir_source_vartype>] -> []
pub fn encode_store_global(
    wasm_globalidx: &[wasmgen::GlobalIdx],
    ir_dest_vartype: ir::VarType,
    ir_source_vartype: ir::VarType,
    expr_builder: &mut wasmgen::ExprBuilder,
) {
    if ir_dest_vartype == ir_source_vartype {
        match ir_dest_vartype {
            ir::VarType::Any | ir::VarType::Func => {
                assert!(wasm_globalidx.len() == 2);
                expr_builder.global_set(wasm_globalidx[0]);
                expr_builder.global_set(wasm_globalidx[1]);
            }
            ir::VarType::Number | ir::VarType::Boolean | ir::VarType::String => {
                assert!(wasm_globalidx.len() == 1);
                expr_builder.global_set(wasm_globalidx[0]);
            }
            ir::VarType::StructT { typeidx: _ } => {
                assert!(wasm_globalidx.len() == 1);
                expr_builder.global_set(wasm_globalidx[0]);
            }
            ir::VarType::Undefined => {
                assert!(wasm_globalidx.len() == 0);
            }
            ir::VarType::Unassigned => {
                panic!("ICE: IR->Wasm: Local static vartype cannot be unassigned");
            }
        }
    } else if ir_dest_vartype == ir::VarType::Any {
        // writing from a specific type to the Any type
        assert!(wasm_globalidx.len() == 2);
        match ir_source_vartype {
            ir::VarType::Any => {
                panic!("ICE");
            }
            ir::VarType::Undefined => {
                expr_builder.i32_const(ir_source_vartype.tag());
                expr_builder.global_set(wasm_globalidx[0]);
            }
            ir::VarType::Unassigned => {
                panic!("ICE: IR->Wasm: Cannot assign to global from unassigned value");
            }
            ir::VarType::Number => {
                expr_builder.i32_const(ir_source_vartype.tag());
                expr_builder.global_set(wasm_globalidx[0]);
                expr_builder.i64_reinterpret_f64(); // convert f64 to i64
                expr_builder.global_set(wasm_globalidx[1]);
            }
            ir::VarType::Boolean | ir::VarType::String => {
                expr_builder.i32_const(ir_source_vartype.tag());
                expr_builder.global_set(wasm_globalidx[0]);
                expr_builder.i64_extend_i32_u(); // convert i32 to i64
                expr_builder.global_set(wasm_globalidx[1]);
            }
            ir::VarType::StructT { typeidx: _ } => {
                expr_builder.i32_const(ir_source_vartype.tag());
                expr_builder.global_set(wasm_globalidx[0]);
                expr_builder.i64_extend_i32_u(); // convert i32 to i64
                expr_builder.global_set(wasm_globalidx[1]);
            }
            ir::VarType::Func => {
                expr_builder.i32_const(ir_source_vartype.tag());
                expr_builder.global_set(wasm_globalidx[0]);
                // the rest of the instructions concats the two i32s from the stack into the i64 global
                expr_builder.i64_extend_i32_u(); // convert i32 to i64 (index in table)
                expr_builder.global_set(wasm_globalidx[1]);
                expr_builder.i64_extend_i32_u(); // convert i32 to i64 (ptr to closure)
                expr_builder.i64_const(32);
                expr_builder.i64_shl();
                expr_builder.global_get(wasm_globalidx[1]);
                expr_builder.i64_or();
                expr_builder.global_set(wasm_globalidx[1]);
            }
        }
    } else {
        panic!("ICE: IR->Wasm: Assignment to global is not equivalent or widening conversion");
    }
}

// stores a ir variable from the protected stack to a location in memory
// net wasm stack: [struct_ptr, <irvartype>] -> []
pub fn encode_store_memory(
    wasm_struct_offset: u32,
    ir_dest_vartype: ir::VarType,
    ir_source_vartype: ir::VarType,
    scratch: &mut Scratch,
    expr_builder: &mut wasmgen::ExprBuilder,
) {
    if ir_dest_vartype == ir_source_vartype {
        match ir_dest_vartype {
            ir::VarType::Any => {
                let localidx_tag: wasmgen::LocalIdx = scratch.push_i32();
                let localidx_data: wasmgen::LocalIdx = scratch.push_i64();
                let localidx_ptr: wasmgen::LocalIdx = scratch.push_i32();
                expr_builder.local_set(localidx_tag);
                expr_builder.local_set(localidx_data);
                expr_builder.local_tee(localidx_ptr);
                expr_builder.local_get(localidx_tag);
                expr_builder.i32_store(wasmgen::MemArg::new4(wasm_struct_offset));
                expr_builder.local_get(localidx_ptr);
                expr_builder.local_get(localidx_data);
                expr_builder.i64_store(wasmgen::MemArg::new4(wasm_struct_offset + 4));
                scratch.pop_i32();
                scratch.pop_i64();
                scratch.pop_i32();
            }
            ir::VarType::Unassigned => {
                panic!("ICE: IR->Wasm: Cannot assign from unassigned value");
            }
            ir::VarType::Undefined => {}
            ir::VarType::Number => {
                expr_builder.f64_store(wasmgen::MemArg::new4(wasm_struct_offset));
            }
            ir::VarType::Boolean => {
                expr_builder.i32_store(wasmgen::MemArg::new4(wasm_struct_offset));
            }
            ir::VarType::String => {
                expr_builder.i32_store(wasmgen::MemArg::new4(wasm_struct_offset));
            }
            ir::VarType::Func => {
                let localidx_tableidx: wasmgen::LocalIdx = scratch.push_i32();
                let localidx_closure: wasmgen::LocalIdx = scratch.push_i32();
                let localidx_ptr: wasmgen::LocalIdx = scratch.push_i32();
                expr_builder.local_set(localidx_tableidx);
                expr_builder.local_set(localidx_closure);
                expr_builder.local_tee(localidx_ptr);
                expr_builder.local_get(localidx_tableidx);
                expr_builder.i32_store(wasmgen::MemArg::new4(wasm_struct_offset));
                expr_builder.local_get(localidx_ptr);
                expr_builder.local_get(localidx_closure);
                expr_builder.i32_store(wasmgen::MemArg::new4(wasm_struct_offset + 4));
                scratch.pop_i32();
                scratch.pop_i32();
                scratch.pop_i32();
            }
            ir::VarType::StructT { typeidx: _ } => {
                expr_builder.i32_store(wasmgen::MemArg::new4(wasm_struct_offset));
            }
        }
    } else if ir_dest_vartype == ir::VarType::Any {
        // writing from a specific type to the Any type
        match ir_source_vartype {
            ir::VarType::Any => {
                panic!("ICE");
            }
            ir::VarType::Unassigned => {
                panic!("ICE: IR->Wasm: Cannot assign from unassigned value");
            }
            ir::VarType::Undefined => {
                expr_builder.i32_const(ir_source_vartype.tag());
                expr_builder.i32_store(wasmgen::MemArg::new4(wasm_struct_offset));
            }
            ir::VarType::Number => {
                let localidx_val: wasmgen::LocalIdx = scratch.push_f64();
                let localidx_ptr: wasmgen::LocalIdx = scratch.push_i32();
                expr_builder.local_set(localidx_val);
                expr_builder.local_tee(localidx_ptr);
                expr_builder.i32_const(ir_source_vartype.tag());
                expr_builder.i32_store(wasmgen::MemArg::new4(wasm_struct_offset));
                expr_builder.local_get(localidx_ptr);
                expr_builder.local_get(localidx_val);
                expr_builder.f64_store(wasmgen::MemArg::new4(wasm_struct_offset + 4));
                scratch.pop_i32();
                scratch.pop_f64();
            }
            ir::VarType::Boolean | ir::VarType::String => {
                let localidx_val: wasmgen::LocalIdx = scratch.push_i32();
                let localidx_ptr: wasmgen::LocalIdx = scratch.push_i32();
                expr_builder.local_set(localidx_val);
                expr_builder.local_tee(localidx_ptr);
                expr_builder.i32_const(ir_source_vartype.tag());
                expr_builder.i32_store(wasmgen::MemArg::new4(wasm_struct_offset));
                expr_builder.local_get(localidx_ptr);
                expr_builder.local_get(localidx_val);
                expr_builder.i32_store(wasmgen::MemArg::new4(wasm_struct_offset + 4)); // note: high bytes of memory not used
                scratch.pop_i32();
                scratch.pop_i32();
            }
            ir::VarType::StructT { typeidx: _ } => {
                let localidx_val: wasmgen::LocalIdx = scratch.push_i32();
                let localidx_ptr: wasmgen::LocalIdx = scratch.push_i32();
                expr_builder.local_set(localidx_val);
                expr_builder.local_tee(localidx_ptr);
                expr_builder.i32_const(ir_source_vartype.tag());
                expr_builder.i32_store(wasmgen::MemArg::new4(wasm_struct_offset));
                expr_builder.local_get(localidx_ptr);
                expr_builder.local_get(localidx_val);
                expr_builder.i32_store(wasmgen::MemArg::new4(wasm_struct_offset + 4)); // note: high bytes of memory not used
                scratch.pop_i32();
                scratch.pop_i32();
            }
            ir::VarType::Func => {
                let localidx_tableidx: wasmgen::LocalIdx = scratch.push_i32();
                let localidx_closure: wasmgen::LocalIdx = scratch.push_i32();
                let localidx_ptr: wasmgen::LocalIdx = scratch.push_i32();
                expr_builder.local_set(localidx_tableidx);
                expr_builder.local_set(localidx_closure);
                expr_builder.local_tee(localidx_ptr);
                expr_builder.i32_const(ir_source_vartype.tag());
                expr_builder.i32_store(wasmgen::MemArg::new4(wasm_struct_offset));
                expr_builder.local_get(localidx_ptr);
                expr_builder.local_get(localidx_tableidx);
                expr_builder.i32_store(wasmgen::MemArg::new4(wasm_struct_offset + 4));
                expr_builder.local_get(localidx_ptr);
                expr_builder.local_get(localidx_closure);
                expr_builder.i32_store(wasmgen::MemArg::new4(wasm_struct_offset + 8));
                scratch.pop_i32();
                scratch.pop_i32();
                scratch.pop_i32();
            }
        }
    } else {
        panic!("ICE: IR->Wasm: Assignment not equivalent or widening conversion");
    }
}

// loads a ir variable into the protected stack from local variable(s)
// net wasm stack: [] -> [<outgoing_vartype>]
pub fn encode_load_local(
    wasm_localidx: &[wasmgen::LocalIdx],
    ir_local_vartype: ir::VarType,
    ir_outgoing_vartype: ir::VarType,
    expr_builder: &mut wasmgen::ExprBuilder,
) {
    if ir_local_vartype == ir_outgoing_vartype {
        match ir_local_vartype {
            ir::VarType::Any | ir::VarType::Func => {
                assert!(wasm_localidx.len() == 2);
                expr_builder.local_get(wasm_localidx[1]);
                expr_builder.local_get(wasm_localidx[0]);
            }
            ir::VarType::Number | ir::VarType::Boolean | ir::VarType::String => {
                assert!(wasm_localidx.len() == 1);
                expr_builder.local_get(wasm_localidx[0]);
            }
            ir::VarType::StructT { typeidx: _ } => {
                assert!(wasm_localidx.len() == 1);
                expr_builder.local_get(wasm_localidx[0]);
            }
            ir::VarType::Undefined => {
                assert!(wasm_localidx.len() == 0);
            }
            ir::VarType::Unassigned => {
                panic!("ICE: IR->Wasm: Local static vartype cannot be unassigned");
            }
        }
    } else if ir_local_vartype == ir::VarType::Any {
        // loading from Any type to a specific type
        assert!(wasm_localidx.len() == 2);
        match ir_outgoing_vartype {
            ir::VarType::Any => {
                panic!("ICE");
            }
            ir::VarType::Undefined => {}
            ir::VarType::Unassigned => {
                panic!("ICE: IR->Wasm: Cannot load from unassigned local");
            }
            ir::VarType::Number => {
                expr_builder.local_get(wasm_localidx[1]);
                expr_builder.f64_reinterpret_i64(); // convert i64 to f64
            }
            ir::VarType::Boolean | ir::VarType::String => {
                expr_builder.local_get(wasm_localidx[1]);
                expr_builder.i32_wrap_i64(); // convert i64 to i32
            }
            ir::VarType::StructT { typeidx: _ } => {
                expr_builder.local_get(wasm_localidx[1]);
                expr_builder.i32_wrap_i64(); // convert i64 to i32
            }
            ir::VarType::Func => {
                // get high bits into i32
                expr_builder.local_get(wasm_localidx[1]);
                expr_builder.i64_const(32);
                expr_builder.i64_shr_u();
                expr_builder.i32_wrap_i64();
                // get low bits into i32
                expr_builder.local_get(wasm_localidx[1]);
                expr_builder.i32_wrap_i64();
            }
        }
    } else {
        panic!("ICE: IR->Wasm: Load from local is not equivalent or narrowing conversion");
    }
}

// loads a ir variable into the protected stack from global variable(s)
// equivalent to encode_load_local() but for globals
// net wasm stack: [] -> [<outgoing_vartype>]
pub fn encode_load_global(
    wasm_globalidx: &[wasmgen::GlobalIdx],
    ir_global_vartype: ir::VarType,
    ir_outgoing_vartype: ir::VarType,
    expr_builder: &mut wasmgen::ExprBuilder,
) {
    if ir_global_vartype == ir_outgoing_vartype {
        match ir_global_vartype {
            ir::VarType::Any | ir::VarType::Func => {
                assert!(wasm_globalidx.len() == 2);
                expr_builder.global_get(wasm_globalidx[1]);
                expr_builder.global_get(wasm_globalidx[0]);
            }
            ir::VarType::Number | ir::VarType::Boolean | ir::VarType::String => {
                assert!(wasm_globalidx.len() == 1);
                expr_builder.global_get(wasm_globalidx[0]);
            }
            ir::VarType::StructT { typeidx: _ } => {
                assert!(wasm_globalidx.len() == 1);
                expr_builder.global_get(wasm_globalidx[0]);
            }
            ir::VarType::Undefined => {
                assert!(wasm_globalidx.len() == 0);
            }
            ir::VarType::Unassigned => {
                panic!("ICE: IR->Wasm: Local static vartype cannot be unassigned");
            }
        }
    } else if ir_global_vartype == ir::VarType::Any {
        // loading from Any type to a specific type
        assert!(wasm_globalidx.len() == 2);
        match ir_outgoing_vartype {
            ir::VarType::Any => {
                panic!("ICE");
            }
            ir::VarType::Undefined => {}
            ir::VarType::Unassigned => {
                panic!("ICE: IR->Wasm: Cannot load from unassigned global");
            }
            ir::VarType::Number => {
                expr_builder.global_get(wasm_globalidx[1]);
                expr_builder.f64_reinterpret_i64(); // convert i64 to f64
            }
            ir::VarType::Boolean | ir::VarType::String => {
                expr_builder.global_get(wasm_globalidx[1]);
                expr_builder.i32_wrap_i64(); // convert i64 to i32
            }
            ir::VarType::StructT { typeidx: _ } => {
                expr_builder.global_get(wasm_globalidx[1]);
                expr_builder.i32_wrap_i64(); // convert i64 to i32
            }
            ir::VarType::Func => {
                // get high bits into i32
                expr_builder.global_get(wasm_globalidx[1]);
                expr_builder.i64_const(32);
                expr_builder.i64_shr_u();
                expr_builder.i32_wrap_i64();
                // get low bits into i32
                expr_builder.global_get(wasm_globalidx[1]);
                expr_builder.i32_wrap_i64();
            }
        }
    } else {
        panic!("ICE: IR->Wasm: Load from local is not equivalent or narrowing conversion");
    }
}

// net wasm stack: [struct_ptr] -> [<outgoing_vartype>]
pub fn encode_load_memory(
    wasm_struct_offset: u32,
    ir_local_vartype: ir::VarType,
    ir_outgoing_vartype: ir::VarType,
    scratch: &mut Scratch,
    expr_builder: &mut wasmgen::ExprBuilder,
) {
    if ir_local_vartype == ir_outgoing_vartype {
        match ir_local_vartype {
            ir::VarType::Any => {
                let localidx_ptr: wasmgen::LocalIdx = scratch.push_i32();
                expr_builder.local_tee(localidx_ptr);
                expr_builder.i64_load(wasmgen::MemArg::new4(wasm_struct_offset + 4));
                expr_builder.local_get(localidx_ptr);
                expr_builder.i32_load(wasmgen::MemArg::new4(wasm_struct_offset));
                scratch.pop_i32();
            }
            ir::VarType::Undefined => {}
            ir::VarType::Unassigned => {
                panic!("ICE: IR->Wasm: Cannot load from unassigned memory");
            }
            ir::VarType::Number => {
                expr_builder.f64_load(wasmgen::MemArg::new4(wasm_struct_offset));
            }
            ir::VarType::Boolean => {
                expr_builder.i32_load(wasmgen::MemArg::new4(wasm_struct_offset));
            }
            ir::VarType::String => {
                expr_builder.i32_load(wasmgen::MemArg::new4(wasm_struct_offset));
            }
            ir::VarType::Func => {
                let localidx_ptr: wasmgen::LocalIdx = scratch.push_i32();
                expr_builder.local_tee(localidx_ptr);
                expr_builder.i32_load(wasmgen::MemArg::new4(wasm_struct_offset + 4));
                expr_builder.local_get(localidx_ptr);
                expr_builder.i32_load(wasmgen::MemArg::new4(wasm_struct_offset));
                scratch.pop_i32();
            }
            ir::VarType::StructT { typeidx: _ } => {
                expr_builder.i32_load(wasmgen::MemArg::new4(wasm_struct_offset));
            }
        }
    } else if ir_local_vartype == ir::VarType::Any {
        match ir_outgoing_vartype {
            ir::VarType::Any => {
                panic!("ICE");
            }
            ir::VarType::Unassigned => {
                panic!("ICE: IR->Wasm: Cannot load from unassigned memory");
            }
            ir::VarType::Undefined => {}
            ir::VarType::Number => {
                expr_builder.f64_load(wasmgen::MemArg::new4(wasm_struct_offset + 4));
            }
            ir::VarType::Boolean | ir::VarType::String => {
                expr_builder.i32_load(wasmgen::MemArg::new4(wasm_struct_offset + 4));
                // note: high bytes of memory not used
            }
            ir::VarType::StructT { typeidx: _ } => {
                expr_builder.i32_load(wasmgen::MemArg::new4(wasm_struct_offset + 4));
                // note: high bytes of memory not used
            }
            ir::VarType::Func => {
                let localidx_ptr: wasmgen::LocalIdx = scratch.push_i32();
                expr_builder.local_tee(localidx_ptr);
                expr_builder.i32_load(wasmgen::MemArg::new4(wasm_struct_offset + 8));
                expr_builder.local_get(localidx_ptr);
                expr_builder.i32_load(wasmgen::MemArg::new4(wasm_struct_offset + 4));
                scratch.pop_i32();
            }
        }
    } else {
        panic!("ICE: IR->Wasm: Load from memory is not equivalent or narrowing conversion");
    }
}

// net wasm stack: [<source_type>] -> [<target_type>]
pub fn encode_widening_operation(
    target_type: ir::VarType,
    source_type: ir::VarType,
    scratch: &mut Scratch,
    expr_builder: &mut wasmgen::ExprBuilder,
) {
    if target_type == source_type {
        // The widening operation is a no-op, because the source type is the same as the target type
    } else if target_type == ir::VarType::Any {
        // We are widening from a specific type to Any
        match source_type {
            ir::VarType::Any => {
                panic!("ICE");
            }
            ir::VarType::Undefined => {
                expr_builder.i64_const(0); // unused data
                expr_builder.i32_const(source_type.tag());
            }
            ir::VarType::Unassigned => {
                panic!("ICE: IR->Wasm: Cannot push to stack from unassigned value");
            }
            ir::VarType::Number => {
                expr_builder.i64_reinterpret_f64(); // convert f64 to i64
                expr_builder.i32_const(source_type.tag());
            }
            ir::VarType::Boolean | ir::VarType::String => {
                expr_builder.i64_extend_i32_u(); // convert i32 to i64
                expr_builder.i32_const(source_type.tag());
            }
            ir::VarType::StructT { typeidx: _ } => {
                expr_builder.i64_extend_i32_u(); // convert i32 to i64
                expr_builder.i32_const(source_type.tag());
            }
            ir::VarType::Func => {
                let localidx_tableidx: wasmgen::LocalIdx = scratch.push_i32();
                expr_builder.local_set(localidx_tableidx);
                expr_builder.i64_extend_i32_u(); // convert i32 to i64 (ptr to closure)
                expr_builder.i64_const(32);
                expr_builder.i64_shl();
                expr_builder.local_get(localidx_tableidx);
                expr_builder.i64_extend_i32_u(); // convert i32 to i64 (index in table)
                expr_builder.i64_or();
                expr_builder.i32_const(source_type.tag());
                scratch.pop_i32();
            }
        }
    } else {
        panic!("Widening operation target not a supertype of source");
    }
}

// net wasm stack: [<source_type>] -> [<target_type>]
pub fn encode_narrowing_operation<F: FnOnce(&mut wasmgen::ExprBuilder)>(
    target_type: ir::VarType,
    source_type: ir::VarType,
    failure_encoder: F,
    scratch: &mut Scratch,
    expr_builder: &mut wasmgen::ExprBuilder,
) {
    if target_type == source_type {
        // The narrowing operation is a no-op, because the source type is the same as the target type
    } else if source_type == ir::VarType::Any {
        // We are narrowing from Any to a specific type
        // emit type check (trap if not correct)

        // net wasm stack: [i64(data), i32(tag)] -> [i64(data)]
        expr_builder.i32_const(target_type.tag());
        expr_builder.i32_ne();
        expr_builder.if_(&[]);
        failure_encoder(expr_builder);
        expr_builder.end();

        // now the i64(data) is guaranteed to actually contain the target source_type
        // net wasm stack: [i64(data)] -> [<target_type>]
        match target_type {
            ir::VarType::Any => {
                panic!("ICE");
            }
            ir::VarType::Undefined => {
                expr_builder.drop(); // i64(data) unused
            }
            ir::VarType::Unassigned => {
                panic!("ICE: IR->Wasm: Cannot push unassigned value to stack");
            }
            ir::VarType::Number => {
                expr_builder.f64_reinterpret_i64(); // convert i64 to f64
            }
            ir::VarType::Boolean | ir::VarType::String | ir::VarType::StructT { typeidx: _ } => {
                expr_builder.i32_wrap_i64(); // convert i64 to i32
            }
            ir::VarType::Func => {
                let localidx_data: wasmgen::LocalIdx = scratch.push_i64();
                expr_builder.local_tee(localidx_data);
                expr_builder.i64_const(32);
                expr_builder.i64_shr_u();
                expr_builder.i32_wrap_i64();
                expr_builder.local_get(localidx_data);
                expr_builder.i32_wrap_i64();
                scratch.pop_i64();
            }
        }
    } else {
        panic!("Narrowing operation source not a supertype of target");
    }
}

// A very very specific function that converts an i64 local (the data of an Any) to locals representing a specific type.
// It is used in the TypeCast expression
// net wasm stack: [] -> []
pub fn encode_unchecked_local_conv_any_narrowing(
    wasm_source_localidx: wasmgen::LocalIdx,
    wasm_dest_localidx: &[wasmgen::LocalIdx],
    ir_dest_vartype: ir::VarType,
    _scratch: &mut Scratch,
    expr_builder: &mut wasmgen::ExprBuilder,
) {
    match ir_dest_vartype {
        ir::VarType::Any => {
            panic!("ICE: IR->Wasm: Cannot TypeCast from Any to Any");
        }
        ir::VarType::Undefined => {}
        ir::VarType::Unassigned => {
            panic!("ICE: IR->Wasm: Cannot TypeCast from Any to Unassigned");
        }
        ir::VarType::Number => {
            assert!(wasm_dest_localidx.len() == 1);
            expr_builder.local_get(wasm_source_localidx);
            expr_builder.f64_reinterpret_i64(); // convert i64 to f64
            expr_builder.local_set(wasm_dest_localidx[0]);
        }
        ir::VarType::Boolean | ir::VarType::String | ir::VarType::StructT { typeidx: _ } => {
            assert!(wasm_dest_localidx.len() == 1);
            expr_builder.local_get(wasm_source_localidx);
            expr_builder.i32_wrap_i64(); // convert i64 to i32
            expr_builder.local_set(wasm_dest_localidx[0]);
        }
        ir::VarType::Func => {
            assert!(wasm_dest_localidx.len() == 2);
            expr_builder.local_get(wasm_source_localidx);
            expr_builder.i32_wrap_i64();
            expr_builder.local_set(wasm_dest_localidx[0]); // function ptr
            expr_builder.local_get(wasm_source_localidx);
            expr_builder.i64_const(32);
            expr_builder.i64_shr_u();
            expr_builder.i32_wrap_i64();
            expr_builder.local_set(wasm_dest_localidx[1]); // closure ptr
        }
    }
}

pub fn size_in_memory(ir_vartype: ir::VarType) -> u32 {
    match ir_vartype {
        ir::VarType::Any => 4 + 8,
        ir::VarType::Unassigned => 0,
        ir::VarType::Undefined => 0,
        ir::VarType::Number => 8,
        ir::VarType::Boolean => 4,
        ir::VarType::String => 4,
        ir::VarType::Func => 4 + 4,
        ir::VarType::StructT { typeidx: _ } => 4,
    }
}
