use crate::basic_block::{handler_for_block, BasicBlock};
use crate::cil::StaticFieldDescriptor;
use crate::cil_tree::cil_node::CILNode;
use crate::cil_tree::cil_root::CILRoot;
use crate::cil_tree::CILTree;
use crate::method::MethodType;
use crate::rustc_middle::dep_graph::DepContext;
use crate::utilis::field_descrptor;
use crate::{
    access_modifier::AccessModifer, cil::CallSite, codegen_error::CodegenError,
    codegen_error::MethodCodegenError, function_sig::FnSig, method::Method, r#type::TyCache,
    r#type::Type, r#type::TypeDef, IString,
};
use crate::{call, conv_isize, conv_usize, ldc_u32, ldc_u64};
use rustc_middle::mir::interpret::Allocation;
use rustc_middle::mir::{
    interpret::{AllocId, GlobalAlloc},
    mono::MonoItem,
    Local, LocalDecl, Statement, Terminator,
};
use rustc_middle::ty::{Instance, ParamEnv, TyCtxt, TyKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
/// Data representing a reference to an external assembly.
pub struct AssemblyExternRef {
    /// A tuple describing the referenced assebmly.
    /// Tuple contains:
    /// (Major Version, Minor Version, Revision number, Build number)
    /// In that order.
    version: (u16, u16, u16, u16),
}
impl AssemblyExternRef {
    /// Returns the version information of this assembly.
    #[must_use]
    pub fn version(&self) -> (u16, u16, u16, u16) {
        self.version
    }
}
#[derive(Serialize, Deserialize, Debug)]
/// Representation of a .NET assembly.
pub struct Assembly {
    /// List of types desined within the assembly.
    types: HashMap<IString, TypeDef>,
    /// List of functions defined within this assembly.
    functions: HashMap<CallSite, Method>,
    /// Callsite representing the entrypoint of this assebmly if any present.
    entrypoint: Option<CallSite>,
    /// List of references to external assemblies
    extern_refs: HashMap<IString, AssemblyExternRef>,
    extern_fns: HashMap<(IString, FnSig), IString>,
    /// List of all static fields within the assembly
    static_fields: HashMap<IString, Type>,
}
impl Assembly {
    /// Returns iterator over all global fields
    pub fn globals(&self) -> impl Iterator<Item = (&IString, &Type)> {
        self.static_fields.iter()
    }
    /// Returns the `.cctor` function used to initialize static data
    #[must_use]
    pub fn cctor(&self) -> Option<&Method> {
        self.functions.get(&CallSite::new(
            None,
            ".cctor".into(),
            FnSig::new(&[], &Type::Void),
            true,
        ))
    }
    /// Returns the external assembly reference
    #[must_use]
    pub fn extern_refs(&self) -> &HashMap<IString, AssemblyExternRef> {
        &self.extern_refs
    }
    /// Creates a new, empty assembly.
    #[must_use]
    pub fn empty() -> Self {
        let mut res = Self {
            types: HashMap::new(),
            functions: HashMap::new(),
            entrypoint: None,
            extern_refs: HashMap::new(),
            static_fields: HashMap::new(),
            extern_fns: HashMap::new(),
        };
        let dotnet_ver = AssemblyExternRef {
            version: (6, 12, 0, 0),
        };
        res.extern_refs.insert("System.Runtime".into(), dotnet_ver);
        //res.extern_refs.insert("mscorlib".into(),dotnet_ver);
        res.extern_refs
            .insert("System.Runtime.InteropServices".into(), dotnet_ver);
        // Needed to get C-Mode to work
        res.add_cctor();
        res
    }
    /// Joins 2 assemblies together.
    #[must_use]
    pub fn join(self, other: Self) -> Self {
        let static_initializer = link_static_initializers(self.cctor(), other.cctor());
        let mut types = self.types;
        types.extend(other.types);
        let mut functions = self.functions;
        functions.extend(other.functions);
        if let Some(static_initializer) = static_initializer {
            functions.insert(static_initializer.call_site(), static_initializer);
        }
        let entrypoint = self.entrypoint.or(other.entrypoint);
        let mut extern_refs = self.extern_refs;
        let mut static_fields = self.static_fields;
        let mut extern_fns = self.extern_fns;
        static_fields.extend(other.static_fields);
        extern_refs.extend(other.extern_refs);
        extern_fns.extend(other.extern_fns);
        Self {
            types,
            functions,
            entrypoint,
            extern_refs,
            extern_fns,
            static_fields,
        }
    }
    /// Gets the typdefef at path `path`.
    #[must_use]
    pub fn get_typedef_by_path(&self, path: &str) -> Option<&TypeDef> {
        if path.contains('/') {
            let mut path_iter = path.split('/');
            let path_first = path_iter.next().unwrap();
            let mut td = self.get_typedef_by_path(path_first)?;
            // FIXME: this loop is messy.
            for part in path_iter {
                let old = td;
                for inner in td.inner_types() {
                    if inner.name() == part {
                        td = inner;
                        break;
                    }
                }
                if td == old {
                    return None;
                }
            }
            return Some(td);
        }
        self.types()
            .find(|&tpe| tpe.0.as_ref() == path)
            .map(|t| t.1)
    }
    /// Turns a terminator into ops, if `ABORT_ON_ERROR` set to false, will handle and recover from errors.
    pub fn terminator_to_ops<'tcx>(
        term: &Terminator<'tcx>,
        mir: &'tcx rustc_middle::mir::Body<'tcx>,
        tcx: TyCtxt<'tcx>,
        instance: Instance<'tcx>,
        type_cache: &mut TyCache,
    ) -> Vec<CILTree> {
        let terminator = if *crate::config::ABORT_ON_ERROR {
            crate::terminator::handle_terminator(term, mir, tcx, mir, instance, type_cache)
        } else {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::terminator::handle_terminator(term, mir, tcx, mir, instance, type_cache)
            })) {
                Ok(ok) => ok,
                Err(payload) => {
                    type_cache.recover_from_panic();
                    let msg = if let Some(msg) = payload.downcast_ref::<&str>() {
                        rustc_middle::ty::print::with_no_trimmed_paths! {
                        format!("Tried to execute terminator {term:?} whose compialtion message {msg:?}!")}
                    } else {
                        eprintln!("handle_terminator panicked with a non-string message!");
                        rustc_middle::ty::print::with_no_trimmed_paths! {
                        format!("Tried to execute terminator {term:?} whose compialtion failed with a no-string message!")
                        }
                    };
                    CILRoot::throw(&msg).into()
                }
            }
        };
        /*
        if !crate::utilis::verify_locals_within_range(&terminator, argc, locc) {
            let msg = rustc_middle::ty::print::with_no_trimmed_paths! {format!("{term:?} failed verification, because it refered to local varibles/arguments that do not exist. ops:{terminator:?} argc:{argc} locc:{locc}")};
            eprintln!("WARING: teminator {msg}");
            terminator.clear();
            rustc_middle::ty::print::with_no_trimmed_paths! {terminator.extend(CILOp::throw_msg(&format!(
                "Tried to execute miscompiled terminator {term:?}, which {msg}"
            )))};
        }*/
        terminator
    }
    /// Turns a statement into ops, if `ABORT_ON_ERROR` set to false, will handle and recover from errors.
    pub fn statement_to_ops<'tcx>(
        statement: &Statement<'tcx>,
        tcx: TyCtxt<'tcx>,
        mir: &rustc_middle::mir::Body<'tcx>,
        instance: Instance<'tcx>,
        type_cache: &mut TyCache,
    ) -> Result<Option<CILTree>, CodegenError> {
        if *crate::config::ABORT_ON_ERROR {
            Ok(crate::statement::handle_statement(
                statement, tcx, mir, instance, type_cache,
            ))
        } else {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::statement::handle_statement(statement, tcx, mir, instance, type_cache)
            })) {
                Ok(success) => Ok(success),
                Err(payload) => {
                    if let Some(msg) = payload.downcast_ref::<&str>() {
                        Err(crate::codegen_error::CodegenError::from_panic_message(msg))
                    } else {
                        Err(crate::codegen_error::CodegenError::from_panic_message(
                            "statement_to_ops panicked with a non-string message!",
                        ))
                    }
                }
            }
        }
    }
    /// This is used *ONLY* to catch uncaught errors.
    fn checked_add_fn<'tcx>(
        &mut self,
        instance: Instance<'tcx>,
        tcx: TyCtxt<'tcx>,
        name: &str,
        cache: &mut TyCache,
    ) -> Result<(), MethodCodegenError> {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.add_fn(instance, tcx, name, cache)
        })) {
            Ok(success) => success,
            Err(payload) => {
                cache.recover_from_panic();
                if let Some(msg) = payload.downcast_ref::<&str>() {
                    eprintln!("could not compile method {name}. fn_add panicked with unhandled message: {msg:?}");
                    //self.add_method(Method::missing_because(format!("could not compile method {name}. fn_add panicked with unhandled message: {msg:?}")));
                    Ok(())
                } else {
                    eprintln!("could not compile method {name}. fn_add panicked with no message.");
                    Ok(())
                }
            }
        }
    }
    //fn terminator_to_ops()
    /// Adds a rust MIR function to the assembly.
    pub fn add_fn<'tyctx>(
        &mut self,
        instance: Instance<'tyctx>,
        tyctx: TyCtxt<'tyctx>,
        name: &str,
        cache: &mut TyCache,
    ) -> Result<(), MethodCodegenError> {
        if crate::utilis::is_function_magic(name) {
            return Ok(());
        }
        if let TyKind::FnDef(_, _) = instance.ty(tyctx, ParamEnv::reveal_all()).kind() {
            //ALL OK.
        } else if let TyKind::Closure(_, _) = instance.ty(tyctx, ParamEnv::reveal_all()).kind() {
            //println!("CLOSURE")
        } else {
            eprintln!("fn item {instance:?} is not a function definition type. Skippping.");
            return Ok(());
        }
        let mir = tyctx.instance_mir(instance.def);
        // Check if function is public or not.
        // FIXME: figure out the source of the bug causing visibility to not be read propely.
        // let access_modifier = AccessModifer::from_visibility(tcx.visibility(instance.def_id()));
        let access_modifier = AccessModifer::Public;
        // Handle the function signature
        let call_site = crate::call_info::CallInfo::sig_from_instance_(instance, tyctx, cache);
        let sig = call_site.sig().clone();

        // Get locals
        //eprintln!("method")
        let (arg_names, mut locals) = locals_from_mir(
            &mir.local_decls,
            tyctx,
            mir.arg_count,
            &instance,
            cache,
            &mir.var_debug_info,
        );

        let blocks = &mir.basic_blocks;
        //let mut trees = Vec::new();
        let mut normal_bbs = Vec::new();
        let mut cleanup_bbs = Vec::new();
        for (last_bb_id, block_data) in blocks.into_iter().enumerate() {
            //ops.push(CILOp::Label(last_bb_id as u32));
            let mut trees = Vec::new();
            for statement in &block_data.statements {
                if *crate::config::INSERT_MIR_DEBUG_COMMENTS {
                    rustc_middle::ty::print::with_no_trimmed_paths! {trees.push(CILRoot::debug(&format!("{statement:?}")).into())};
                }

                let statement_tree = match Self::statement_to_ops(
                    statement, tyctx, mir, instance, cache,
                ) {
                    Ok(ops) => ops,
                    Err(err) => {
                        cache.recover_from_panic();
                        rustc_middle::ty::print::with_no_trimmed_paths! {eprintln!(
                            "Method \"{name}\" failed to compile statement {statement:?} with message {err:?}"
                        )};
                        rustc_middle::ty::print::with_no_trimmed_paths! {Some(CILRoot::throw(&format!("Tired to run a statement {statement:?} which failed to compile with error message {err:?}.")).into())}
                    }
                };
                // Only save debuginfo for statements which result in ops.
                if statement_tree.is_some() {
                    trees.push(CILRoot::span_source_info(tyctx, statement.source_info.span).into());
                }
                trees.extend(statement_tree);

                //crate::utilis::check_debugable(&statement_ops, statement, does_return_void);
                //ops.extend(statement_ops);
                //
            }
            match &block_data.terminator {
                Some(term) => {
                    if *crate::config::INSERT_MIR_DEBUG_COMMENTS {
                        rustc_middle::ty::print::with_no_trimmed_paths! {trees.push(CILRoot::debug(&format!("{term:?}")).into())};
                    }
                    let term_trees = Self::terminator_to_ops(term, mir, tyctx, instance, cache);
                    if !term_trees.is_empty() {
                        trees.push(CILRoot::span_source_info(tyctx, term.source_info.span).into());
                    }
                    trees.extend(term_trees);
                }
                None => (),
            }
            if block_data.is_cleanup {
                cleanup_bbs.push(BasicBlock::new(
                    trees,
                    u32::try_from(last_bb_id).unwrap(),
                    handler_for_block(block_data, &mir.basic_blocks, tyctx, &instance, mir),
                ));
            } else {
                normal_bbs.push(BasicBlock::new(
                    trees,
                    u32::try_from(last_bb_id).unwrap(),
                    handler_for_block(block_data, &mir.basic_blocks, tyctx, &instance, mir),
                ));
            }
            //ops.extend(trees.iter().flat_map(|tree| tree.flatten()))
        }
        if let Some(spread_arg) = mir.spread_arg {
            // Prepare for repacking the argument tuple, by allocating a local
            let repacked =
                u32::try_from(locals.len()).expect("More than 2^32 arguments of a function");
            let repacked_ty: rustc_middle::ty::Ty =
                crate::utilis::monomorphize(&instance, mir.local_decls[spread_arg].ty, tyctx);
            let repacked_type = cache.type_from_cache(repacked_ty, tyctx, Some(instance));
            locals.push((Some("repacked_arg".into()), repacked_type));
            let mut repack_cil = Vec::new();
            // For each element of the tuple, get the argument spread_arg + n
            let packed_count = if let TyKind::Tuple(tup) = repacked_ty.kind() {
                u32::try_from(tup.len()).expect("More than 2^32 arguments of a function")
            } else {
                panic!("Arg to spread not a tuple???")
            };
            for arg_id in 0..packed_count {
                let arg_field = field_descrptor(repacked_ty, arg_id, tyctx, instance, cache);
                repack_cil.push(
                    CILRoot::SetField {
                        addr: CILNode::LDLocA(repacked),
                        value: CILNode::LDArg((spread_arg.as_u32() - 1) + arg_id),
                        desc: arg_field,
                    }
                    .into(),
                );
            }
            // Get the first bb, and append repack_cil at its start
            let first_bb = &mut normal_bbs[0];
            repack_cil.append(first_bb.trees_mut());
            *first_bb.trees_mut() = repack_cil;
        }
        normal_bbs
            .iter_mut()
            .for_each(|bb| bb.resolve_exception_handlers(&cleanup_bbs));

        let mut method = Method::new(
            access_modifier,
            MethodType::Static,
            sig.clone(),
            name,
            locals,
            normal_bbs,
        )
        .with_argnames(arg_names);
        method.resolve_global_allocations(self, tyctx, cache);
        // TODO: Why is this even needed? The temporaries *should* be already allocated, why not all of them are?
        method.allocate_temporaries();
        if *crate::config::TYPECHECK_CIL {
            match method.validate() {
                Ok(()) => (),
                Err(msg) => eprintln!(
                    "\n\nMethod {} failed compilation with message:\ns {msg}",
                    method.name()
                ),
            }
        }

        let adjust = check_align_adjust(&mir.local_decls, tyctx, &instance);
        method.adjust_aligement(adjust);
        self.add_method(method);
        Ok(())
        //todo!("Can't add function")
    }
    /// Adds a global static field named *name* of type *tpe*
    pub fn add_static(&mut self, tpe: Type, name: &str) {
        self.static_fields.insert(name.into(), tpe);
    }
    fn add_cctor(&mut self) -> &mut Method {
        self.functions
            .entry(CallSite::new(
                None,
                ".cctor".into(),
                FnSig::new(&[], &Type::Void),
                true,
            ))
            .or_insert_with(|| {
                Method::new(
                    AccessModifer::Public,
                    MethodType::Static,
                    FnSig::new(&[], &Type::Void),
                    ".cctor",
                    vec![
                        (None, Type::Ptr(Type::U8.into())),
                        (None, Type::Ptr(Type::U8.into())),
                    ],
                    vec![BasicBlock::new(vec![CILRoot::VoidRet.into()], 0, None)],
                )
            })
    }
    /// Adds a static field and initialized for allocation represented by `alloc_id`.
    pub fn add_allocation(
        &mut self,
        alloc_id: u64,
        tcx: TyCtxt<'_>,
        tycache: &mut TyCache,
    ) -> crate::cil::StaticFieldDescriptor {
        let const_allocation =
            match tcx.global_alloc(AllocId(alloc_id.try_into().expect("0 alloc id?"))) {
                GlobalAlloc::Memory(alloc) => alloc,
                GlobalAlloc::Static(def_id) => {
                    let alloc = tcx.eval_static_initializer(def_id).unwrap();
                    //tcx.reserve_and_set_memory_alloc(alloc)
                    alloc
                }
                GlobalAlloc::VTable(..) => {
                    //TODO: handle VTables
                    let alloc_fld: IString = format!("alloc_{alloc_id:x}").into();
                    let field_desc = crate::cil::StaticFieldDescriptor::new(
                        None,
                        Type::Ptr(Type::U8.into()),
                        alloc_fld.clone(),
                    );
                    self.static_fields
                        .insert(alloc_fld, Type::Ptr(Type::U8.into()));
                    return field_desc;
                }
                GlobalAlloc::Function(_) => {
                    //TODO: handle constant functions
                    let alloc_fld: IString = format!("alloc_{alloc_id:x}").into();
                    let field_desc = crate::cil::StaticFieldDescriptor::new(
                        None,
                        Type::Ptr(Type::U8.into()),
                        alloc_fld.clone(),
                    );
                    self.static_fields
                        .insert(alloc_fld, Type::Ptr(Type::U8.into()));
                    return field_desc;
                    //todo!("Function/Vtable allocation.");
                }
            };

        let const_allocation = const_allocation.inner();

        let bytes: &[u8] = const_allocation
            .inspect_with_uninit_and_ptr_outside_interpreter(0..const_allocation.len());
        // Alloc ids are *not* unique across all crates. Adding the hash here ensures we don't overwrite allocations during linking
        // TODO:consider using something better here / making the hashes stable.
        let byte_hash = calculate_hash(&bytes);
        let alloc_fld: IString = format!("alloc_{alloc_id:x}_{byte_hash:x}").into();

        let field_desc = crate::cil::StaticFieldDescriptor::new(
            None,
            Type::Ptr(Type::U8.into()),
            alloc_fld.clone(),
        );
        if !self.static_fields.contains_key(&alloc_fld) {
            let init_method =
                allocation_initializer_method(const_allocation, &alloc_fld, tcx, self, tycache);
            let cctor = self.add_cctor();
            let mut blocks = cctor.blocks_mut();
            if blocks.is_empty() {
                blocks.push(BasicBlock::new(vec![CILRoot::VoidRet.into()], 0, None));
            }
            assert_eq!(
                blocks.len(),
                1,
                "Unexpected number of basic blocks in a static data initializer."
            );
            let trees = blocks[0].trees_mut();
            {
                // Remove return
                let ret = trees.pop().unwrap();
                // Append initailzer
                trees.push(
                    CILRoot::SetStaticField {
                        descr: field_desc.clone(),
                        value: call!(
                            CallSite::new(
                                None,
                                init_method.name().into(),
                                init_method.sig().clone(),
                                true,
                            ),
                            []
                        ),
                    }
                    .into(),
                );
                //trees.push(CILRoot::debug(&format!("Finished initializing allocation {alloc_fld:?}")).into());
                // Add return again
                trees.push(ret);
            }
            drop(blocks);
            self.add_method(init_method);
            self.add_static(Type::Ptr(Type::U8.into()), &alloc_fld);
        }
        field_desc
    }
    /// Returns true if assembly contains function named `name`
    #[must_use]
    pub fn contains_fn_named(&self, name: &str) -> bool {
        //FIXME:This is inefficient.
        self.methods().any(|m| m.name() == name)
    }
    /// Returns true if assembly contains function named `name`
    #[must_use]
    pub fn contains_fn(&self, site: &CallSite) -> bool {
        self.functions.contains_key(site)
    }
    /// Adds a method to the assebmly.
    pub fn add_method(&mut self, mut method: Method) {
        method.allocate_temporaries();
        //method.ensure_valid();
        if *crate::config::VERIFY_METHODS {
            //crate::verify::verify(&method);
        }

        self.functions.insert(method.call_site(), method);
    }
    /// Returns the list of all calls within the method. Calls may repeat.
    #[must_use]
    pub fn call_sites(&self) -> Vec<CallSite> {
        self.methods()
            .flat_map(super::method::Method::calls)
            .collect()
    }
    pub fn remove_dead_statics(&mut self) {
        // Get the set of "alive" fields(fields referenced outside of the static initializer).
        let alive_fields: std::collections::HashSet<_> = self
            .methods()
            .filter(|method| method.name() != ".cctor")
            .flat_map(super::method::Method::sflds)
            .collect();
        // Remove the definitions of all non-alive fields
        self.static_fields.retain(|name, tpe| {
            alive_fields.contains(&StaticFieldDescriptor::new(None, tpe.clone(), name.clone()))
        });
        // Remove their initializers from the cctor
        let Some(cctor) = self.cctor_mut() else {
            return;
        };
        for tree in cctor
            .blocks_mut()
            .iter_mut()
            .flat_map(super::basic_block::BasicBlock::trees_mut)
        {
            if let CILRoot::SetStaticField { descr, value } = tree.root_mut() {
                // Assigement to a dead static, remove.
                if !alive_fields.contains(descr) {
                    debug_assert!(descr.name().contains('a'));
                    debug_assert!(matches!(value, CILNode::Call { site: _, args: _ }));
                    *tree = CILRoot::Nop.into();
                }
            }
        }
    }
    /// Returns an interator over all methods within the assembly.
    pub fn methods(&self) -> impl Iterator<Item = &Method> {
        self.functions.values()
    }
    /// Returns an iterator over all types witin the assembly.
    pub fn types(&self) -> impl Iterator<Item = (&IString, &TypeDef)> {
        self.types.iter()
    }
    /// Optimizes all the methods witin the assembly.
    pub fn opt(&mut self) {
        let functions: HashMap<_, _> = self
            .functions
            .iter()
            .map(|method| {
                let (site, method) = method;
                let mut method = method.clone();
                crate::opt::opt_method(&mut method, self);
                (site.clone(), method)
            })
            .collect();
        self.functions = functions;
    }
    /// Adds a definition of a type to the assembly.
    pub fn add_typedef(&mut self, type_def: TypeDef) {
        self.types.insert(type_def.name().into(), type_def);
    }
    /// Adds a MIR item (method,inline assembly code, etc.) to the assembly.
    pub fn add_item<'tcx>(
        &mut self,
        item: MonoItem<'tcx>,
        tcx: TyCtxt<'tcx>,
        cache: &mut TyCache,
    ) -> Result<(), CodegenError> {
        match item {
            MonoItem::Fn(instance) => {
                //let instance = crate::utilis::monomorphize(&instance,tcx);
                let symbol_name: Box<str> = crate::utilis::function_name(item.symbol_name(tcx));

                let function_compile_timer = tcx.profiler().generic_activity_with_arg(
                    "compile function",
                    item.symbol_name(tcx).to_string(),
                );
                self.checked_add_fn(instance, tcx, &symbol_name, cache)
                    .expect("Could not add function!");
                drop(function_compile_timer);
                Ok(())
            }
            MonoItem::GlobalAsm(asm) => {
                eprintln!("Unsuported item - Global ASM:{asm:?}");
                Ok(())
            }
            MonoItem::Static(stotic) => {
                let static_compile_timer = tcx.profiler().generic_activity_with_arg(
                    "compile static initializer",
                    item.symbol_name(tcx).to_string(),
                );
                let alloc = tcx.eval_static_initializer(stotic).unwrap();
                let alloc_id = tcx.reserve_and_set_memory_alloc(alloc);

                self.add_allocation(crate::utilis::alloc_id_to_u64(alloc_id), tcx, cache);
                //let ty = alloc.0;
                drop(static_compile_timer);
                //eprintln!("Unsuported item - Static:{stotic:?}");
                Ok(())
            }
        }
    }
    /// Sets the entrypoint of the assembly to the method behind `CallSite`.
    pub fn set_entrypoint(&mut self, entrypoint: &CallSite) {
        assert!(self.entrypoint.is_none(), "ERROR: Multiple entrypoints");
        let wrapper = crate::entrypoint::wrapper(entrypoint);
        self.entrypoint = Some(wrapper.call_site());
        self.add_method(wrapper);
    }

    #[must_use]
    pub fn extern_fns(&self) -> &HashMap<(IString, FnSig), IString> {
        &self.extern_fns
    }

    pub fn add_extern_fn(&mut self, name: IString, sig: FnSig, lib: IString) {
        self.extern_fns.insert((name, sig), lib);
    }
    fn get_exported_fn(&self) -> HashMap<CallSite, Method> {
        let mut externs = HashMap::new();
        if let Some(entrypoint) = &self.entrypoint {
            let method = self.functions.get(entrypoint).cloned().unwrap();
            externs.insert(entrypoint.clone(), method);
        }
        if let Some(cctor) = self.cctor() {
            externs.insert(
                CallSite::new(None, ".cctor".into(), FnSig::new(&[], &Type::Void), true),
                cctor.clone(),
            );
        }
        externs
    }
    pub fn eliminate_dead_fn(&mut self) {
        let mut alive: HashMap<CallSite, Method> = HashMap::new();
        let mut resurecting: HashMap<CallSite, Method> = HashMap::new();
        let mut to_resurect: HashMap<CallSite, Method> = self.get_exported_fn();
        while !to_resurect.is_empty() {
            alive.extend(resurecting.clone());
            resurecting.clear();
            resurecting.extend(to_resurect.clone());
            to_resurect.clear();
            for call in resurecting.iter().flat_map(|fnc| fnc.1.calls()) {
                if let Some(_class) = call.class() {
                    // TODO: if dead code elimination too agressive check this
                    // Methods reference by methods inside types are NOT tracked.
                    continue;
                }
                if alive.contains_key(&call) || resurecting.contains_key(&call) {
                    // Already alive, ignore!
                    continue;
                }
                if let Some(method) = self.functions.get(&call).cloned() {
                    to_resurect.insert(call.clone(), method);
                };
            }
        }
        alive.extend(resurecting);
        self.functions = alive;
    }
    pub fn eliminate_dead_code(&mut self) {
        if *crate::config::DEAD_CODE_ELIMINATION {
            self.eliminate_dead_fn();
            self.remove_dead_statics();
            // Call eliminate_dead_fn again, to remove now-dead static initializers.
            self.eliminate_dead_fn();
        }

        //self.eliminate_dead_types();
    }
    pub fn eliminate_dead_types(&mut self) {
        let mut alive = HashMap::new();
        let mut resurected: HashMap<IString, _> = self
            .functions
            .values()
            .flat_map(super::method::Method::dotnet_types)
            .filter_map(|tpe| match tpe.asm() {
                Some(_) => None,
                None => Some(IString::from(tpe.name_path())),
            })
            .map(|name| (name.clone(), self.types.get(&name).unwrap().clone()))
            .collect();
        resurected.insert(
            "RustVoid".into(),
            self.types.get("RustVoid").cloned().unwrap(),
        );
        let mut to_resurect: HashMap<IString, _> = HashMap::new();
        while !resurected.is_empty() {
            for tpe in &resurected {
                alive.insert(tpe.0.clone(), tpe.1.clone());
                for (name, type_def) in tpe
                    .1
                    .all_types()
                    .filter_map(super::r#type::r#type::Type::dotnet_refs)
                    .filter_map(|tpe| match tpe.asm() {
                        Some(_) => None,
                        None => Some(IString::from(tpe.name_path())),
                    })
                    .filter_map(|name| name.split_once('\\').map(|(a, _)| a.into()))
                    //.map(|(a,b)|a.into())
                    .map(|name: IString| {
                        (
                            name.clone(),
                            self.types
                                .get(&name)
                                .unwrap_or_else(|| panic!("Can't find type {name:?}"))
                                .clone(),
                        )
                    })
                {
                    let name: IString = IString::from(name);
                    to_resurect.insert(name, type_def);
                }
            }
            resurected = to_resurect;
            to_resurect = HashMap::new();
        }
        self.types = alive;
    }

    pub fn cctor_mut(&mut self) -> Option<&mut Method> {
        self.functions.get_mut(&CallSite::new(
            None,
            ".cctor".into(),
            FnSig::new(&[], &Type::Void),
            true,
        ))
    }

    pub(crate) fn add_const_value(
        &mut self,
        bytes: u128,
        tyctx: TyCtxt,
    ) -> crate::cil::StaticFieldDescriptor {
        let alloc_fld: IString = format!("a_{bytes:x}").into();
        let raw_bytes = bytes.to_le_bytes();
        let field_desc = crate::cil::StaticFieldDescriptor::new(
            None,
            Type::Ptr(Type::U8.into()),
            alloc_fld.clone(),
        );
        if !self.static_fields.contains_key(&alloc_fld) {
            let block = BasicBlock::new(
                vec![
                    CILRoot::STLoc {
                        local: 0,
                        tree: call!(CallSite::malloc(tyctx), [ldc_u32!(16)]),
                    }
                    .into(),
                    CILRoot::STIndI8(CILNode::LDLoc(0), ldc_u32!(u32::from(raw_bytes[0]))).into(),
                    CILRoot::STIndI8(
                        CILNode::LDLoc(0) + ldc_u32!(1),
                        ldc_u32!(u32::from(raw_bytes[1])),
                    )
                    .into(),
                    CILRoot::STIndI8(
                        CILNode::LDLoc(0) + ldc_u32!(2),
                        ldc_u32!(u32::from(raw_bytes[2])),
                    )
                    .into(),
                    CILRoot::STIndI8(
                        CILNode::LDLoc(0) + ldc_u32!(3),
                        ldc_u32!(u32::from(raw_bytes[3])),
                    )
                    .into(),
                    CILRoot::STIndI8(
                        CILNode::LDLoc(0) + ldc_u32!(4),
                        ldc_u32!(u32::from(raw_bytes[4])),
                    )
                    .into(),
                    CILRoot::STIndI8(
                        CILNode::LDLoc(0) + ldc_u32!(5),
                        ldc_u32!(u32::from(raw_bytes[5])),
                    )
                    .into(),
                    CILRoot::STIndI8(
                        CILNode::LDLoc(0) + ldc_u32!(6),
                        ldc_u32!(u32::from(raw_bytes[6])),
                    )
                    .into(),
                    CILRoot::STIndI8(
                        CILNode::LDLoc(0) + ldc_u32!(7),
                        ldc_u32!(u32::from(raw_bytes[7])),
                    )
                    .into(),
                    CILRoot::STIndI8(
                        CILNode::LDLoc(0) + ldc_u32!(8),
                        ldc_u32!(u32::from(raw_bytes[8])),
                    )
                    .into(),
                    CILRoot::STIndI8(
                        CILNode::LDLoc(0) + ldc_u32!(9),
                        ldc_u32!(u32::from(raw_bytes[9])),
                    )
                    .into(),
                    CILRoot::STIndI8(
                        CILNode::LDLoc(0) + ldc_u32!(10),
                        ldc_u32!(u32::from(raw_bytes[10])),
                    )
                    .into(),
                    CILRoot::STIndI8(
                        CILNode::LDLoc(0) + ldc_u32!(11),
                        ldc_u32!(u32::from(raw_bytes[11])),
                    )
                    .into(),
                    CILRoot::STIndI8(
                        CILNode::LDLoc(0) + ldc_u32!(12),
                        ldc_u32!(u32::from(raw_bytes[12])),
                    )
                    .into(),
                    CILRoot::STIndI8(
                        CILNode::LDLoc(0) + ldc_u32!(13),
                        ldc_u32!(u32::from(raw_bytes[13])),
                    )
                    .into(),
                    CILRoot::STIndI8(
                        CILNode::LDLoc(0) + ldc_u32!(14),
                        ldc_u32!(u32::from(raw_bytes[14])),
                    )
                    .into(),
                    CILRoot::STIndI8(
                        CILNode::LDLoc(0) + ldc_u32!(15),
                        ldc_u32!(u32::from(raw_bytes[15])),
                    )
                    .into(),
                    CILRoot::Ret {
                        tree: CILNode::LDLoc(0),
                    }
                    .into(),
                ],
                0,
                None,
            );
            let init_method = Method::new(
                AccessModifer::Public,
                MethodType::Static,
                FnSig::new(&[], &Type::Ptr(Type::U8.into())),
                &format!("init_a{bytes:x}"),
                vec![(Some("alloc_ptr".into()), Type::Ptr(Type::U8.into()))],
                vec![block],
            );

            let cctor = self.add_cctor();
            let mut blocks = cctor.blocks_mut();
            if blocks.is_empty() {
                blocks.push(BasicBlock::new(vec![CILRoot::VoidRet.into()], 0, None));
            }
            assert_eq!(
                blocks.len(),
                1,
                "Unexpected number of basic blocks in a static data initializer."
            );
            let trees = blocks[0].trees_mut();
            {
                // Remove return
                let ret = trees.pop().unwrap();
                // Append initailzer
                trees.push(
                    CILRoot::SetStaticField {
                        descr: StaticFieldDescriptor::new(
                            None,
                            Type::Ptr(Type::U8.into()),
                            alloc_fld.clone(),
                        )
                        .clone(),
                        value: call!(
                            CallSite::new(
                                None,
                                init_method.name().into(),
                                init_method.sig().clone(),
                                true,
                            ),
                            []
                        ),
                    }
                    .into(),
                );
                // Add return again
                trees.push(ret);
            }
            drop(blocks);
            self.add_method(init_method);
            self.add_static(Type::Ptr(Type::U8.into()), &alloc_fld);
        }
        field_desc
    }
}
fn link_static_initializers(a: Option<&Method>, b: Option<&Method>) -> Option<Method> {
    match (a, b) {
        (None, None) => None,
        (Some(a), None) => Some(a.clone()),
        (None, Some(b)) => Some(b.clone()),
        (Some(a), Some(b)) => {
            let mut merged: Method = a.clone();
            let mut blocks = merged.blocks_mut();
            let trees = blocks[0].trees_mut();
            trees.pop();
            trees.extend(b.blocks()[0].trees().iter().cloned());
            drop(blocks);
            Some(merged)
        }
    }
}
type LocalDefList = Vec<(Option<IString>, Type)>;
type ArgsDebugInfo = Vec<Option<IString>>;
fn check_align_adjust<'tyctx>(locals: &rustc_index::IndexVec<Local, LocalDecl<'tyctx>>,
tyctx: TyCtxt<'tyctx>,method_instance:&Instance<'tyctx>,)->Vec<Option<u64>>{
    let mut adjusts:Vec<Option<u64>> = Vec::with_capacity(locals.len());
    for (_, local) in locals.iter().enumerate() {
        let ty = crate::utilis::monomorphize(method_instance, local.ty, tyctx);
        let adjust = crate::utilis::requries_align_adjustement(ty,tyctx,);
        if let Some(adjust) = adjust{
            eprintln!(
                "type {ty} requires algiement adjustements. Its algement should be {adjust:?}.",
            );
        }
       
        adjusts.push(adjust);
    }
    adjusts
}
/// Returns the list of all local variables within MIR of a function, and converts them to the internal type represenation `Type`
fn locals_from_mir<'tyctx>(
    locals: &rustc_index::IndexVec<Local, LocalDecl<'tyctx>>,
    tyctx: TyCtxt<'tyctx>,
    argc: usize,
    method_instance: &Instance<'tyctx>,
    tycache: &mut TyCache,
    var_debuginfo: &[rustc_middle::mir::VarDebugInfo<'tyctx>],
) -> (ArgsDebugInfo, LocalDefList) {
    use rustc_middle::mir::VarDebugInfoContents;
    let mut local_types: Vec<(Option<IString>, _)> = Vec::with_capacity(locals.len());
    for (local_id, local) in locals.iter().enumerate() {
        if local_id == 0 || local_id > argc {
            let ty = crate::utilis::monomorphize(method_instance, local.ty, tyctx);
            if *crate::config::PRINT_LOCAL_TYPES {
                println!(
                    "Local type {ty:?},non-morphic: {non_morph}",
                    non_morph = local.ty
                );
            }
            let name = None;
            let tpe = tycache.type_from_cache(ty, tyctx, Some(*method_instance));
            local_types.push((name, tpe));
        }
    }
    let mut arg_names: Vec<Option<IString>> = (0..argc).map(|_| None).collect();
    for var in var_debuginfo {
        let mir_local = match var.value {
            VarDebugInfoContents::Place(place) => {
                // Check if this is just a "naked" local(eg. just a local varaible, with no indirction)
                if !place.projection.is_empty() {
                    continue;
                }
                place.local.as_usize()
            }
            VarDebugInfoContents::Const(_) => continue,
        };
        if mir_local == 0 {
            local_types[0].0 = Some(var.name.to_string().into());
        } else if mir_local > argc {
            local_types[mir_local - argc].0 = Some(var.name.to_string().into());
        } else {
            arg_names[mir_local - 1] = Some(var.name.to_string().into());
        }
    }
    (arg_names, local_types)
}

fn allocation_initializer_method(
    const_allocation: &Allocation,
    name: &str,
    tyctx: TyCtxt,
    asm: &mut Assembly,
    tycache: &mut TyCache,
) -> Method {
    let bytes: &[u8] =
        const_allocation.inspect_with_uninit_and_ptr_outside_interpreter(0..const_allocation.len());
    let ptrs = const_allocation.provenance().ptrs();
    let mut trees: Vec<CILTree> = Vec::new();
    let align = const_allocation.align.bytes().max(1);
    //trees.push(CILRoot::debug(&format!("Preparing to initialize allocation with size {}",bytes.len())).into());
    trees.push(
        CILRoot::STLoc {
            local: 0,
            tree: CILNode::TransmutePtr {
                val: Box::new(call!(
                    CallSite::alloc(),
                    [conv_isize!(ldc_u64!(bytes.len() as u64)),conv_isize!(ldc_u64!(align as u64))]
                )),
                new_ptr: Box::new(Type::Ptr(Box::new(Type::U8))),
            },
        }
        .into(),
    );
    trees.push(
        CILRoot::STLoc {
            local: 1,
            tree: CILNode::LDLoc(0),
        }
        .into(),
    );
    for byte in bytes {
        trees.push(CILRoot::STIndI8(CILNode::LDLoc(0), ldc_u32!(u32::from(*byte))).into());
        //trees.push(CILRoot::debug(&format!("Writing the byte {}",byte)).into());
        trees.push(
            CILRoot::STLoc {
                local: 0,
                tree: CILNode::LDLoc(0) + conv_usize!(ldc_u32!(1)),
            }
            .into(),
        );
    }
    if !ptrs.is_empty() {
        for (offset, prov) in ptrs.iter() {
            let offset = u32::try_from(offset.bytes_usize()).unwrap();
            // Check if this allocation is a function
            let reloc_target_alloc = tyctx.global_alloc(prov.alloc_id());
            if let GlobalAlloc::Function(finstance) = reloc_target_alloc {
                // If it is a function, patch its pointer up.
                let call_info =
                    crate::call_info::CallInfo::sig_from_instance_(finstance, tyctx, tycache);
                let function_name = crate::utilis::function_name(tyctx.symbol_name(finstance));

                trees.push(
                    CILRoot::STIndISize(
                        CILNode::LDLoc(1) + conv_usize!(ldc_u32!(offset)),
                        CILNode::LDFtn(
                            CallSite::new(None, function_name, call_info.sig().clone(), true)
                                .into(),
                        ),
                    )
                    .into(),
                );
            } else {
                let ptr_alloc = asm.add_allocation(prov.alloc_id().0.into(), tyctx, tycache);

                trees.push(
                    CILRoot::STIndISize(
                        CILNode::LDLoc(1) + conv_usize!(ldc_u32!(offset)),
                        CILNode::LDStaticField(ptr_alloc.into()),
                    )
                    .into(),
                );
            }
        }
        //eprintln!("Constant requires rellocation support!");
    }
    //trees.push(CILRoot::debug(&format!("Finished initializing an allocation with size {}",bytes.len())).into());
    trees.push(
        CILRoot::Ret {
            tree: CILNode::LDLoc(1),
        }
        .into(),
    );

    Method::new(
        AccessModifer::Private,
        MethodType::Static,
        FnSig::new(&[], &Type::Ptr(Type::U8.into())),
        &format!("init_{name}"),
        vec![
            (Some("curr".into()), Type::Ptr(Type::U8.into())),
            (Some("alloc_ptr".into()), Type::Ptr(Type::U8.into())),
        ],
        vec![BasicBlock::new(trees, 0, None)],
    )
}
fn calculate_hash<T: std::hash::Hash>(t: &T) -> u64 {
    use std::hash::DefaultHasher;
    use std::hash::Hasher;
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}
