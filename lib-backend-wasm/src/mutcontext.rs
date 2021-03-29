use crate::func::ModuleEncodeWrapper;
use crate::gc::HeapManager;
use crate::var_conv::*;
use wasmgen::ExprBuilder;
use wasmgen::Scratch;

use std::vec::Vec;

/**
 * Manages all the local variables and types.  It also interfaces with the GC since it knows which locals are not in the gc roots yet.
 */

pub struct MutContext<'a, 'b> {
    // Local to this function
    scratch: Scratch<'a>,
    // note: "shadow locals" are temporary ir locals that are generated as part of codegen
    // (such as when evaluating a complicated expression that might allocate memory so temporary values need to be
    // registered in the mutctx so that the gc will know about them)
    // scratch - contains all wasm locals, and know their types
    // So if you have a named local i, to get the actual wasm locals,
    // you should do wasm_local_map[local_map[names_local_map[i]]..(local_map[names_local_map[i]+1)]]
    // wasm locals that are not in the wasm_local_map are auxiliary scratch space (e.g. for type conversion procedures)
    // locals that are not in the named local map are auxiliary ir locals (e.g. temporary unnamed variables in complex ir expressions)
    //
    // By convention, a parameter prefixed with "named_" is usd as an index into named_local_map,
    // otherwise it should bypass the named_local_map and be used for local_map/local_types directly.
    wasm_local_map: Vec<wasmgen::LocalIdx>,
    local_map: Vec<usize>, // map from ir param/local index (including shadow locals) to wasm_local_map index
    local_types: Vec<ir::VarType>, // map from ir param/local index (including shadow locals) to ir param type
    named_local_map: Vec<usize>, // map from real named local (i.e. those that exist in source code) to local_map/local_type index
    // information for calculating and encoding Break exprs
    ir_landings: Vec<(usize, ir::VarType, Box<[wasmgen::LocalIdx]>)>, // first item of the pair is the landing index (1-based), can be equal to (but no more than) wasm_landing_count
    wasm_landing_count: usize,
    // Global for whole program
    module_wrapper: ModuleEncodeWrapper<'b>,
    // will also include function indices
    temp_array_length: usize,
}
impl<'a, 'b> MutContext<'a, 'b> {
    pub fn new(
        scratch: Scratch<'a>,
        wasm_local_map: &[wasmgen::LocalIdx],
        local_map: &[usize],
        local_types: &[ir::VarType],
        module_wrapper: ModuleEncodeWrapper<'b>,
    ) -> Self {
        let num_locals = local_map.len();
        Self {
            scratch: scratch,
            wasm_local_map: wasm_local_map.to_vec(),
            local_map: local_map.to_vec(),
            local_types: local_types.to_vec(),
            named_local_map: (0..num_locals).collect(),
            ir_landings: Vec::new(),
            wasm_landing_count: 0,
            module_wrapper: module_wrapper,
            temp_array_length: 0,
        }
    }
    /**
     * Adds a shadow local to the context, storing it in scratch, and updating wasm_local_map, local_map, local_types appropriately.
     * Then initializes the locals with wasm_local_roots_init
     * Then runs the given callback f(mutctx, expr_builder, idx) , where `idx` is the index into `local_map` and `local_types` of the new local.
     * Then removes the local from the context but undoing everything.
     */
    pub fn with_shadow_local<
        H: HeapManager,
        R,
        F: FnOnce(&mut MutContext<'a, 'b>, &mut ExprBuilder, usize) -> R,
    >(
        &mut self,
        ir_vartype: ir::VarType,
        heap: &H,
        expr_builder: &mut ExprBuilder,
        f: F,
    ) -> R {
        let idx = self.local_types.len();
        self.push_local(ir_vartype);
        heap.encode_local_roots_init(
            &self.local_types[idx..],
            &self.local_map[idx..],
            &self.wasm_local_map,
            &mut self.scratch,
            expr_builder,
        );
        let result = f(self, expr_builder, idx);
        self.pop_local(ir_vartype);
        result
    }
    /**
     * Like with_shadow_local(), but adds an entry to the named_local_map.
     * The index yielded in the callback is a *named* local idx, which should be equivalent to localidxs used in the ir.
     */
    pub fn with_named_local<
        H: HeapManager,
        R,
        F: FnOnce(&mut MutContext<'a, 'b>, &mut ExprBuilder, usize) -> R,
    >(
        &mut self,
        ir_vartype: ir::VarType,
        heap: &H,
        expr_builder: &mut ExprBuilder,
        f: F,
    ) -> R {
        self.with_shadow_local(
            ir_vartype,
            heap,
            expr_builder,
            move |mutctx, expr_builder, ir_localidx| {
                let named_ir_localidx = mutctx.named_local_map.len();
                mutctx.named_local_map.push(ir_localidx);
                let ret = f(mutctx, expr_builder, named_ir_localidx);
                mutctx.named_local_map.pop();
                ret
            },
        )
    }
    /**
     * Adds an uninitialized shadow local to the context.  It is like with_local(), but caller must guarantee that something is assigned to it before doing any heap allocations.
     */
    pub fn with_uninitialized_shadow_local<R, F: FnOnce(&mut MutContext<'a, 'b>, usize) -> R>(
        &mut self,
        ir_vartype: ir::VarType,
        f: F,
    ) -> R {
        let idx = self.local_types.len();
        self.push_local(ir_vartype);
        let result = f(self, idx);
        self.pop_local(ir_vartype);
        result
    }
    /**
     * Like with_uninitialized_shadow_local(), but adds an entry to the named_local_map.
     * The index yielded in the callback is a *named* local idx, which should be equivalent to localidxs used in the ir.
     */
    pub fn with_uninitialized_named_local<R, F: FnOnce(&mut MutContext<'a, 'b>, usize) -> R>(
        &mut self,
        ir_vartype: ir::VarType,
        f: F,
    ) -> R {
        self.with_uninitialized_shadow_local(ir_vartype, move |mutctx, ir_localidx| {
            let named_ir_localidx = mutctx.named_local_map.len();
            mutctx.named_local_map.push(ir_localidx);
            let ret = f(mutctx, named_ir_localidx);
            mutctx.named_local_map.pop();
            ret
        })
    }
    /**
     * Like `with_local()` but with many locals.
     * f(mutctx, expr_builder, idx), where `idx` is the starting index into `local_map` and `local_types` of the new locals.
     */
    #[allow(dead_code)]
    pub fn with_shadow_locals<
        H: HeapManager,
        R,
        F: FnOnce(&mut MutContext<'a, 'b>, &mut ExprBuilder, usize) -> R,
    >(
        &mut self,
        ir_vartypes: &[ir::VarType],
        heap: &H,
        expr_builder: &mut ExprBuilder,
        f: F,
    ) -> R {
        let idx = self.local_types.len();
        for ir_vartype in ir_vartypes {
            self.push_local(*ir_vartype);
        }
        heap.encode_local_roots_init(
            &self.local_types[idx..],
            &self.local_map[idx..],
            &self.wasm_local_map,
            &mut self.scratch,
            expr_builder,
        );
        let result = f(self, expr_builder, idx);
        for ir_vartype in ir_vartypes.iter().rev() {
            self.pop_local(*ir_vartype);
        }
        result
    }
    /**
     * Like `with_uninitialized_local()` but with many locals.
     * f(mutctx, idx), where `idx` is the starting index into `local_map` and `local_types` of the new locals.
     */
    pub fn with_uninitialized_shadow_locals<R, F: FnOnce(&mut MutContext<'a, 'b>, usize) -> R>(
        &mut self,
        ir_vartypes: &[ir::VarType],
        f: F,
    ) -> R {
        let idx = self.local_types.len();
        for ir_vartype in ir_vartypes {
            self.push_local(*ir_vartype);
        }
        let result = f(self, idx);
        for ir_vartype in ir_vartypes.iter().rev() {
            self.pop_local(*ir_vartype);
        }
        result
    }
    /**
     * Adds an uninitialized local.  This function is dangerous, use with_uninitialized_local if possible.
     */
    pub fn add_uninitialized_shadow_local(&mut self, ir_vartype: ir::VarType) -> usize {
        let ret = self.local_types.len();
        self.push_local(ir_vartype);
        ret
    }
    /**
     * Removes an uninitialized local.  This function is dangerous, use with_uninitialized_local if possible.
     */
    pub fn remove_shadow_local(&mut self, ir_vartype: ir::VarType) {
        self.pop_local(ir_vartype);
    }
    /**
     * Adds a local to the context, storing it in scratch, and updating wasm_local_map, local_map, local_types appropriately.
     * It is uninitialized.
     */
    fn push_local(&mut self, ir_vartype: ir::VarType) {
        assert!(self.local_types.len() == self.local_map.len());
        self.local_types.push(ir_vartype);
        self.local_map.push(self.wasm_local_map.len());
        let wasm_valtypes = encode_vartype(ir_vartype);
        for wasm_valtype in wasm_valtypes {
            let localidx = self.scratch.push(*wasm_valtype);
            self.wasm_local_map.push(localidx);
        }
    }
    /**
     * Undoes the corresponding push_local().  Locals are pushed and popped like a stack.
     */
    fn pop_local(&mut self, ir_vartype: ir::VarType) {
        assert!(self.local_types.len() == self.local_map.len());
        let wasm_valtypes = encode_vartype(ir_vartype);
        for wasm_valtype in wasm_valtypes {
            self.wasm_local_map.pop();
            self.scratch.pop(*wasm_valtype);
        }
        assert!(self.local_map.last().copied() == Some(self.wasm_local_map.len()));
        self.local_map.pop();
        assert!(self.local_types.last().copied() == Some(ir_vartype));
        self.local_types.pop();
    }

    pub fn named_local_types_elem(&self, named_idx: usize) -> ir::VarType {
        self.local_types[self.named_local_map[named_idx]]
    }
    pub fn scratch_mut(&mut self) -> &mut Scratch<'a> {
        &mut self.scratch
    }

    pub fn wasm_local_slice(&self, ir_localidx: usize) -> &[wasmgen::LocalIdx] {
        &self.wasm_local_map[self.local_map[ir_localidx]
            ..(if ir_localidx + 1 < self.local_map.len() {
                self.local_map[ir_localidx + 1]
            } else {
                self.wasm_local_map.len()
            })]
    }
    pub fn named_wasm_local_slice(&self, named_ir_localidx: usize) -> &[wasmgen::LocalIdx] {
        let ir_localidx = self.named_local_map[named_ir_localidx];
        &self.wasm_local_map[self.local_map[ir_localidx]
            ..(if ir_localidx + 1 < self.local_map.len() {
                self.local_map[ir_localidx + 1]
            } else {
                self.wasm_local_map.len()
            })]
    }
    // Same as wasm_local_slice() and scratch_mut() combined, but plays nice with the lifetime checker.
    pub fn named_wasm_local_slice_and_scratch(
        &mut self,
        named_ir_localidx: usize,
    ) -> (&[wasmgen::LocalIdx], &mut Scratch<'a>) {
        let ir_localidx = self.named_local_map[named_ir_localidx];
        (
            &self.wasm_local_map[self.local_map[ir_localidx]
                ..(if ir_localidx + 1 < self.local_map.len() {
                    self.local_map[ir_localidx + 1]
                } else {
                    self.wasm_local_map.len()
                })],
            &mut self.scratch,
        )
    }
    pub fn with_scratch_i32<R, F: FnOnce(&mut Self, wasmgen::LocalIdx) -> R>(&mut self, f: F) -> R {
        let idx = self.scratch.push_i32();
        let result = f(self, idx);
        self.scratch.pop_i32();
        result
    }
    pub fn with_scratch_i64<R, F: FnOnce(&mut Self, wasmgen::LocalIdx) -> R>(&mut self, f: F) -> R {
        let idx = self.scratch.push_i64();
        let result = f(self, idx);
        self.scratch.pop_i64();
        result
    }
    #[allow(dead_code)]
    pub fn with_scratch_f32<R, F: FnOnce(&mut Self, wasmgen::LocalIdx) -> R>(&mut self, f: F) -> R {
        let idx = self.scratch.push_f32();
        let result = f(self, idx);
        self.scratch.pop_f32();
        result
    }
    pub fn with_scratch_f64<R, F: FnOnce(&mut Self, wasmgen::LocalIdx) -> R>(&mut self, f: F) -> R {
        let idx = self.scratch.push_f64();
        let result = f(self, idx);
        self.scratch.pop_f64();
        result
    }
    #[allow(dead_code)]
    pub fn with_scratch<R, F: FnOnce(&mut Self, wasmgen::LocalIdx) -> R>(
        &mut self,
        _valtype: wasmgen::ValType,
        f: F,
    ) -> R {
        let idx = self.scratch_mut().push_f64();
        let result = f(self, idx);
        self.scratch_mut().pop_f64();
        result
    }
    pub fn with_scratches<R, F: FnOnce(&mut Self, &[wasmgen::LocalIdx]) -> R>(
        &mut self,
        valtypes: &[wasmgen::ValType],
        f: F,
    ) -> R {
        let idxs: Box<[wasmgen::LocalIdx]> = valtypes
            .iter()
            .map(|valtype| self.scratch_mut().push(*valtype))
            .collect();
        let result = f(self, &idxs);
        valtypes
            .iter()
            .rev()
            .for_each(|valtype| self.scratch_mut().pop(*valtype));
        result
    }

    /**
     * Adds a ir-landable point
     */
    pub fn with_landing<R, F: FnOnce(&mut MutContext<'a, 'b>) -> R>(
        &mut self,
        ir_vartype: ir::VarType,
        landing_ctx: &[wasmgen::LocalIdx],
        f: F,
    ) -> R {
        self.wasm_landing_count += 1;
        self.ir_landings.push((
            self.wasm_landing_count,
            ir_vartype,
            landing_ctx.to_owned().into_boxed_slice(),
        ));
        let result = f(self);
        self.ir_landings.pop();
        self.wasm_landing_count -= 1;
        result
    }
    /**
     * Adds a non-landable landing point
     */
    pub fn with_unused_landing<R, F: FnOnce(&mut MutContext<'a, 'b>) -> R>(&mut self, f: F) -> R {
        self.wasm_landing_count += 1;
        let result = f(self);
        self.wasm_landing_count -= 1;
        result
    }
    /**
     * Gets the wasm landing index from the given ir landing index.
     * Note that both the ir and wasm indices are relative to the current code.
     */
    pub fn get_wasm_landing(
        &self,
        ir_landing: usize,
    ) -> (usize, ir::VarType, Box<[wasmgen::LocalIdx]>) {
        let (wasm_abs_landing, vartype, landing_ctx) =
            self.ir_landings[self.ir_landings.len() - ir_landing - 1].clone(); // unnecessary clone, but we need to satisfy the Rust type checker
        (
            self.wasm_landing_count - wasm_abs_landing,
            vartype,
            landing_ctx,
        )
    }

    pub fn module_wrapper(&mut self) -> &mut ModuleEncodeWrapper<'b> {
        &mut self.module_wrapper
    }

    // net wasm stack: [] -> [i32(ptr)]
    pub fn heap_encode_fixed_allocation<H: HeapManager>(
        &mut self,
        heap: &H,
        vartype: ir::VarType,
        expr_builder: &mut ExprBuilder,
    ) {
        heap.encode_fixed_allocation(
            vartype,
            &self.local_types,
            &self.local_map,
            &self.wasm_local_map,
            &mut self.scratch,
            expr_builder,
        );
    }
    // net wasm stack: [i32(num_bytes)] -> [i32(ptr)]
    pub fn heap_encode_dynamic_allocation<H: HeapManager>(
        &mut self,
        heap: &H,
        vartype: ir::VarType,
        expr_builder: &mut ExprBuilder,
    ) {
        let temp_length = self.temp_array_length(); // Need to copy this out because of rust's borrow checker
        heap.encode_dynamic_allocation(
            vartype,
            &self.local_types,
            &self.local_map,
            &self.wasm_local_map,
            &mut self.scratch,
            expr_builder,
            temp_length,
        );
    }
    pub fn heap_encode_prologue_epilogue<
        H: HeapManager,
        R,
        F: FnOnce(&mut Self, &mut ExprBuilder) -> R,
    >(
        &mut self,
        heap: &H,
        expr_builder: &mut ExprBuilder,
        f: F,
    ) -> R {
        // encode local roots prologue
        heap.encode_local_roots_prologue(
            &self.local_types,
            &self.local_map,
            &self.wasm_local_map,
            &mut self.scratch,
            expr_builder,
        );

        // do the callback (usually, it is calling a function)
        let result = f(self, expr_builder);

        // encode local roots prologue
        heap.encode_local_roots_epilogue(
            &self.local_types,
            &self.local_map,
            &self.wasm_local_map,
            &mut self.scratch,
            expr_builder,
        );

        result
    }

    pub fn set_temp_array_length(&mut self, new_array_length: usize) {
        assert!(
            self.temp_array_length == 0,
            "ICE: Data race setting array length"
        );
        self.temp_array_length = new_array_length;
    }

    pub fn temp_array_length(&self) -> usize {
        self.temp_array_length
    }
    
    pub fn reset_temp_array_length(&mut self) {
        assert!(
            self.temp_array_length != 0,
            "ICE: Data race setting array length"
        );
        self.temp_array_length = 0;
    }
}
