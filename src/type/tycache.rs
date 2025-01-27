use super::{tuple_name, tuple_typedef, DotnetTypeRef, Type, TypeDef};
use crate::{
    access_modifier::AccessModifer,
    function_sig::FnSig,
    r#type::{closure_typedef, escape_field_name},
    utilis::adt::FieldOffsetIterator,
    IString,
};
use rustc_middle::ty::{
    AdtDef, AdtKind, GenericArg, Instance, List, ParamEnv, Ty, TyCtxt, TyKind, UintTy,
};
use rustc_span::def_id::DefId;
use std::collections::HashMap;
// CAN'T BE SERAILIZED!
pub struct TyCache {
    type_def_cache: HashMap<IString, TypeDef>,
    cycle_prevention: Vec<IString>,
    ptr_components: Option<DefId>,
}
fn create_typedef<'tyctx>(
    _cache: &mut TyCache,
    _name: &str,
    def: AdtDef<'tyctx>,
    _tyctx: TyCtxt<'tyctx>,
    _method: Option<Instance<'tyctx>>,
) -> TypeDef {
    assert_eq!(
        def.adt_kind(),
        AdtKind::Struct,
        "Only struct types may be used in custom .NET typedefs!"
    );
    for _field in def.all_fields() {}
    todo!()
}
impl TyCache {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            type_def_cache: HashMap::new(),
            cycle_prevention: vec![],
            ptr_components: None,
        }
    }
    pub fn defs(&self) -> impl Iterator<Item = &TypeDef> {
        self.type_def_cache.values()
    }
    #[must_use]
    /// Gets the definition ID of the `PtrComponents` type.
    /// # Panics
    /// Will panic if the `PtrComponents` type is missing.
    pub fn ptr_components(&mut self, tyctx: TyCtxt) -> DefId {
        if self.ptr_components.is_none() {
            self.ptr_components = Some(try_find_ptr_components(tyctx));
        }
        self.ptr_components
            .expect("Could not find `PtrComponents`.")
    }
    fn adt<'tyctx>(
        &mut self,
        name: &str,
        def: AdtDef<'tyctx>,
        adt_ty: Ty<'tyctx>,
        subst: &'tyctx List<rustc_middle::ty::GenericArg<'tyctx>>,
        tyctx: TyCtxt<'tyctx>,
        method: Option<Instance<'tyctx>>,
    ) -> DotnetTypeRef {
        if self.type_def_cache.contains_key(name) {
            return DotnetTypeRef::new(None, name);
        }
        if self
            .cycle_prevention
            .iter()
            .any(|c_name| c_name.as_ref() == name)
        {
            return DotnetTypeRef::new(None, name);
        }
        self.cycle_prevention.push(name.into());
        if crate::r#type::is_name_magic(name) {
            assert!(
                subst.is_empty(),
                "A custom typedef may not contain neiter generic arguments nor lifetimes!"
            );
            let def = create_typedef(self, name, def, tyctx, method);
            self.type_def_cache.insert(name.into(), def);
        } else {
            let def = match def.adt_kind() {
                AdtKind::Struct => self.struct_(name, def, adt_ty, subst, tyctx, method),
                AdtKind::Enum => self.enum_(name, def, adt_ty, subst, tyctx, method),
                AdtKind::Union => self.union_(name, def, adt_ty, subst, tyctx, method),
            };
            self.type_def_cache.insert(name.into(), def);
        }
        self.cycle_prevention.pop();
        DotnetTypeRef::new(None, name)
    }
    pub fn recover_from_panic(&mut self) {
        self.cycle_prevention.clear();
    }
    fn struct_<'tyctx>(
        &mut self,
        name: &str,
        adt: AdtDef<'tyctx>,
        adt_ty: Ty<'tyctx>,
        subst: &'tyctx List<rustc_middle::ty::GenericArg<'tyctx>>,
        tyctx: TyCtxt<'tyctx>,
        method: Option<Instance<'tyctx>>,
    ) -> TypeDef {
        if name.contains(super::type_def::CUSTOM_INTEROP_TYPE_DEF) {
            todo!("Can't yet handle custom typedefs!")
        }
        let mut fields = Vec::new();
        for field in &adt
            .variant(rustc_target::abi::VariantIdx::from_u32(0))
            .fields
        {
            let name = escape_field_name(&field.name.to_string());
            let mut field_ty = field.ty(tyctx, subst);
            method.inspect(|method_instance| {
                field_ty = crate::utilis::monomorphize(method_instance, field_ty, tyctx);
            });
            let field_ty = self.type_from_cache(field_ty, tyctx, method);
            fields.push((name, field_ty));
        }

        let access = AccessModifer::Public;
        let layout = tyctx
            .layout_of(rustc_middle::ty::ParamEnvAnd {
                param_env: ParamEnv::reveal_all(),
                value: adt_ty,
            })
            .expect("Could not get type layout!");

        let explicit_offsets =
            crate::utilis::adt::FieldOffsetIterator::fields((*layout.layout.0).clone()).collect();
        //let to_string = create_to_string(adt, subst, adt_ty, self, method, tyctx);
        TypeDef::new(
            access,
            name.into(),
            vec![],
            fields,
            vec![],
            Some(explicit_offsets),
            0,
            None,
            Some(layout.layout.size().bytes()),
        )
    }
    fn union_<'tyctx>(
        &mut self,
        name: &str,
        adt: AdtDef<'tyctx>,
        adt_ty: Ty<'tyctx>,
        subst: &'tyctx List<rustc_middle::ty::GenericArg<'tyctx>>,
        tyctx: TyCtxt<'tyctx>,
        method: Option<Instance<'tyctx>>,
    ) -> TypeDef {
        let mut fields = Vec::new();
        for field in adt.all_fields() {
            let name = escape_field_name(&field.name.to_string());
            let mut field_ty = field.ty(tyctx, subst);
            method.inspect(|method_instance| {
                field_ty = crate::utilis::monomorphize(method_instance, field_ty, tyctx);
            });
            let field_ty = self.type_from_cache(field_ty, tyctx, method);
            fields.push((name, field_ty));
        }

        let access = AccessModifer::Public;
        let layout = tyctx
            .layout_of(rustc_middle::ty::ParamEnvAnd {
                param_env: ParamEnv::reveal_all(),
                value: adt_ty,
            })
            .expect("Could not get type layout!");
        let explicit_offsets =
            crate::utilis::adt::FieldOffsetIterator::fields((*layout.layout.0).clone()).collect();

        TypeDef::new(
            access,
            name.into(),
            vec![],
            fields,
            vec![],
            Some(explicit_offsets),
            0,
            None,
            Some(layout.layout.size().bytes()),
        )
    }
    fn enum_<'tyctx>(
        &mut self,
        enum_name: &str,
        adt: AdtDef<'tyctx>,
        adt_ty: Ty<'tyctx>,
        subst: &'tyctx List<rustc_middle::ty::GenericArg<'tyctx>>,
        tyctx: TyCtxt<'tyctx>,
        method: Option<Instance<'tyctx>>,
    ) -> TypeDef {
        let access = AccessModifer::Public;
        let mut explicit_offsets: Vec<u32> = vec![];

        let layout = tyctx
            .layout_of(rustc_middle::ty::ParamEnvAnd {
                param_env: ParamEnv::reveal_all(),
                value: adt_ty,
            })
            .expect("Could not get type layout!");
        let mut fields = vec![];
   
        match &layout.variants {
            rustc_target::abi::Variants::Single { index: _ } => {
                let (tag_type, offset) = crate::utilis::adt::enum_tag_info(layout.layout, tyctx);
                fields.push(("value__".into(), tag_type));
                explicit_offsets.push(0);
                offset
            }
            rustc_target::abi::Variants::Multiple {
                tag: _,
                tag_encoding,
                tag_field: _,
                variants: _,
            } => {
                let layout = tyctx
                    .layout_of(rustc_middle::ty::ParamEnvAnd {
                        param_env: ParamEnv::reveal_all(),
                        value: adt_ty,
                    })
                    .expect("Could not get type layout!");

                match tag_encoding {
                    rustc_target::abi::TagEncoding::Direct => {
                        let (tag_type, offset) =
                            crate::utilis::adt::enum_tag_info(layout.layout, tyctx);

                        if tag_type != Type::Void {
                            fields.push(("value__".into(), tag_type));
                            explicit_offsets.push(offset);
                        }
                        offset
                    }
                    rustc_target::abi::TagEncoding::Niche {
                        untagged_variant: _,
                        niche_variants: _,
                        ..
                    } => {
                        let (tag_type, offset) =
                            crate::utilis::adt::enum_tag_info(layout.layout, tyctx);
                        let offsets = FieldOffsetIterator::fields((*layout.layout.0).clone());
                        //eprintln!("enum:{adt_ty} layout.fields:{:?}",layout.fields);
                        assert!(offsets.count() > 0, "layout.fields:{:?}", layout.fields);
                        if tag_type != Type::Void {
                            fields.push(("value__".into(), tag_type));

                            explicit_offsets.push(offset);
                        }
                        offset
                    }
                }

                //todo!("Mult-variant enum!"),
            }
        };
        assert_eq!(fields.len(), explicit_offsets.len());
        for (vidx, variant) in adt.variants().iter_enumerated() {
            let variant_name: IString = variant.name.to_string().into();
            let mut variant_fields = vec![];
            for field in &variant.fields {
                let name = format!(
                    "{variant_name}_{fname}",
                    fname = escape_field_name(&field.name.to_string())
                )
                .into();
                let field_ty = self.type_from_cache(field.ty(tyctx, subst), tyctx, method);
                variant_fields.push((name, field_ty));
            }

            let field_offset_iter =
                crate::utilis::adt::enum_variant_offsets(adt, layout.layout, vidx);
            let mut field_offsets: Vec<_> = field_offset_iter.collect();
            // FIXME: this is a hacky fix for `std::option::Option<std::convert::Infallible>`. If an enum contains an enum without variants, stuff breaks(no offset for that field).
            // If we know this is `Option` we can just sweep the issue under the rug and pretend it does not happen(even tough it does).
            if field_offsets.len() < variant_fields.len()
                && ((enum_name.contains("Option") && enum_name.contains("option"))
                    || (enum_name.contains("Result") && enum_name.contains("result")))
            {
                field_offsets.push(0);
            }
            rustc_middle::ty::print::with_no_trimmed_paths! {assert_eq!(field_offsets.len(),variant_fields.len(),"Layout:{:?}", &layout)};
            fields.extend(variant_fields);
            explicit_offsets.extend(field_offsets);
        }
        assert_eq!(fields.len(), explicit_offsets.len());
        TypeDef::new(
            access,
            enum_name.into(),
            vec![],
            fields,
            vec![],
            Some(explicit_offsets),
            0,
            None,
            Some(layout.layout.size().bytes()),
        )
    }
    pub fn slice_ty<'tyctx>(
        &mut self,
        inner: Ty<'tyctx>,
        tyctx: TyCtxt<'tyctx>,
        method: Option<Instance<'tyctx>>,
    ) -> Type {
        slice_ref_to(tyctx, self, Ty::new_slice(tyctx, inner), method)
    }
    /// Converts a [`Ty`] to a dotnet-compatible [`Type`]. It is cached.
    /// # Panics
    /// Will panic if type invalid/unsuported.
    pub fn type_from_cache<'tyctx>(
        &mut self,
        ty: Ty<'tyctx>,
        tyctx: TyCtxt<'tyctx>,
        method: Option<Instance<'tyctx>>,
    ) -> Type {
        match ty.kind() {
            TyKind::Bool => Type::Bool,
            TyKind::Int(int) => int.into(),
            TyKind::Uint(uint) => uint.into(),
            TyKind::Char => Type::U32,
            TyKind::Float(float) => float.into(),
            TyKind::Tuple(types) => {
                let types: Vec<_> = types
                    .iter()
                    .map(|ty| self.type_from_cache(ty, tyctx, method))
                    .collect();
                if types.is_empty() {
                    Type::Void
                } else {
                    let name = tuple_name(&types);
                    let layout = tyctx
                        .layout_of(rustc_middle::ty::ParamEnvAnd {
                            param_env: ParamEnv::reveal_all(),
                            value: ty,
                        })
                        .expect("Could not get type layout!");
                    self.type_def_cache
                        .entry(name)
                        .or_insert_with(|| tuple_typedef(&types, layout.layout));
                    super::simple_tuple(&types).into()
                }
            }
            TyKind::Closure(def, args) => {
                let closure = args.as_closure();
                let mut sig = closure.sig();
                method.inspect(|method| sig = crate::utilis::monomorphize(method, sig, tyctx));
                ////FIXME: This should be OK(since the signature is monomorphized and we don't care about lifetimes anyway), but it would be nice to have a better solution for this.
                let sig = tyctx.normalize_erasing_late_bound_regions(ParamEnv::reveal_all(), sig);
                let inputs: Box<_> = sig
                    .inputs()
                    .iter()
                    .map(|ty| self.type_from_cache(*ty, tyctx, method))
                    .collect();

                let output = self.type_from_cache(sig.output(), tyctx, method);
                let sig = FnSig::new(&inputs, &output);
                let fields: Box<[_]> = closure
                    .upvar_tys()
                    .iter()
                    .map(|ty| self.type_from_cache(ty, tyctx, method))
                    .collect();
                let name: IString = crate::r#type::closure_name(*def, &fields, &sig).into();
                let layout = tyctx
                    .layout_of(rustc_middle::ty::ParamEnvAnd {
                        param_env: ParamEnv::reveal_all(),
                        value: ty,
                    })
                    .expect("Could not get type layout!");
                if !self.type_def_cache.contains_key(&name) {
                    self.type_def_cache.insert(
                        name.clone(),
                        closure_typedef(*def, &fields, &sig, layout.layout),
                    );
                }
                DotnetTypeRef::new(None, &name).into()
            }
            TyKind::Never => Type::Void,
            TyKind::RawPtr(typ, _) => {
                if super::pointer_to_is_fat(*typ, tyctx, method) {
                    let inner = match typ.kind() {
                        TyKind::Slice(inner) => {
                            if let Some(method) = method {
                                crate::utilis::monomorphize(&method, *inner, tyctx)
                            } else {
                                *inner
                            }
                        }
                        TyKind::Str => u8_ty(tyctx),
                        _ => {
                            if let Some(method) = method {
                                crate::utilis::monomorphize(&method, *typ, tyctx)
                            } else {
                                *typ
                            }
                        }
                    };
                    slice_ref_to(tyctx, self, Ty::new_slice(tyctx, inner), method)
                } else {
                    Type::Ptr(self.type_from_cache(*typ, tyctx, method).into())
                }
            }
            TyKind::Adt(def, subst) => {
                let name = crate::utilis::adt_name(*def, tyctx, subst);
                if super::is_name_magic(name.as_ref()) {
                    return super::magic_type(name.as_ref(), def, subst, tyctx);
                }
                self.adt(&name, *def, ty, subst, tyctx, method).into()
            }

            TyKind::Ref(_region, inner, _mut) => {
                if super::pointer_to_is_fat(*inner, tyctx, method) {
                    let inner = match inner.kind() {
                        TyKind::Slice(inner) => {
                            if let Some(method) = method {
                                crate::utilis::monomorphize(&method, *inner, tyctx)
                            } else {
                                *inner
                            }
                        }
                        TyKind::Str => u8_ty(tyctx),
                        _ => {
                            if let Some(method) = method {
                                crate::utilis::monomorphize(&method, *inner, tyctx)
                            } else {
                                *inner
                            }
                        }
                    };
                    slice_ref_to(tyctx, self, Ty::new_slice(tyctx, inner), method)
                } else {
                    Type::Ptr(self.type_from_cache(*inner, tyctx, method).into())
                }
            }
            // Slice type is almost never refered to directly, and should pop up here ONLY in the case of
            // a DST.
            TyKind::Str => Type::U8,
            TyKind::Slice(inner) => {
                let inner = if let Some(method) = method {
                    crate::utilis::monomorphize(&method, *inner, tyctx)
                } else {
                    *inner
                };
                self.type_from_cache(inner, tyctx, method)
            }
            TyKind::Foreign(foregin) => {
                println!("foregin:{foregin:?}");
                Type::Foreign
            }
            TyKind::Bound(_, _inner) => Type::Foreign,
            TyKind::FnPtr(sig) => {
                let sig = FnSig::from_poly_sig(method, tyctx, self, *sig);
                Type::DelegatePtr(sig.into())
            }
            TyKind::FnDef(_did, _subst) => {
                /*
                let subst = if let Some(method) = method {
                    crate::utilis::monomorphize(&method, *subst, tyctx)
                } else {
                    subst
                };
                let instance = Instance::resolve(tyctx, ParamEnv::reveal_all(), *did, subst)
                    .expect("Could not get function instance due to error")
                    .expect("Could not get function instance.");
                let function_name = crate::utilis::function_name(tyctx.symbol_name(instance));
                self.type_def_cache.insert(
                    format!("fn_{function_name}").into(),
                    TypeDef::nameonly(&format!("fn_{function_name}")),
                );
                //todo!("Fn def!");
                Type::FnDef(function_name)*/
                Type::Void
            }
            TyKind::Array(element, length) => {
                let mut length = *length;
                method
                    .inspect(|method| length = crate::utilis::monomorphize(method, length, tyctx));
                let length: usize = crate::utilis::try_resolve_const_size(length).unwrap();
                let mut element = *element;
                method.inspect(|method| {
                    element = crate::utilis::monomorphize(method, element, tyctx);
                });
                let element = self.type_from_cache(element, tyctx, method);
                let layout = tyctx
                    .layout_of(rustc_middle::ty::ParamEnvAnd {
                        param_env: ParamEnv::reveal_all(),
                        value: ty,
                    })
                    .expect("Could not get type layout!");
                let arr_size = layout.layout.size();
                let arr_name = crate::r#type::type_def::arr_name(length, &element);
                if !self.type_def_cache.contains_key(&arr_name) {
                    self.type_def_cache.insert(
                        arr_name.clone(),
                        crate::r#type::type_def::get_array_type(
                            length,
                            element.clone(),
                            arr_size.bytes(),
                        ),
                    );
                }
                DotnetTypeRef::array(&element, length).into()
            }
            TyKind::Alias(_, _) => {
                //self.cycle_prevention.push("ALIAS_PREV")
                if let Some(method) = method {
                    self.type_from_cache(
                        crate::utilis::monomorphize(&method, ty, tyctx),
                        tyctx,
                        Some(method),
                    )
                } else {
                    panic!("Unmorphized alias {ty:?}")
                }
            }
            _ => todo!("Can't yet get type {ty:?} from type cache."),
        }
    }
}
pub fn slice_ref_to<'tyctx>(
    tyctx: TyCtxt<'tyctx>,
    cache: &mut TyCache,
    mut inner: Ty<'tyctx>,
    method: Option<Instance<'tyctx>>,
) -> Type {
    method.inspect(|method| inner = crate::utilis::monomorphize(method, inner, tyctx));
    let inner = ty_generic_arg(inner);
    // TODO: ensure this function call is valid.
    let list = tyctx.mk_args(&[inner]);

    let ptr_components = cache.ptr_components(tyctx);
    //std::process::exit(-1);
    let adt_def = tyctx.adt_def(ptr_components);
    let ty = Ty::new(tyctx, TyKind::Adt(adt_def, list));
    cache.type_from_cache(ty, tyctx, method)
}
fn u8_ty(tyctx: TyCtxt) -> Ty {
    Ty::new(tyctx, TyKind::Uint(UintTy::U8))
}
/// Turns a `ty` into a `generic_arg`
#[must_use]
pub fn ty_generic_arg(ty: Ty) -> GenericArg {
    // Shit version, ok only cause type tag is 0b00
    unsafe { std::mem::transmute(ty) }
    // Good version
    /*
    rustc_middle::ty::GenericArgKind::Type(ty).pack()
    */
}
// WARING: This function is hacky as shit. It assumes the index of defid of PtrComponents is smaller than the index of the last public function. This *should* almost always be the case,
// but it might not be.

fn try_find_ptr_components(ctx: TyCtxt) -> DefId {
    use crate::rustc_middle::dep_graph::DepContext;
    use rustc_middle::middle::exported_symbols::ExportedSymbol;
    let find_ptr_components_timer = ctx
        .profiler()
        .generic_activity("ptr::metadata::PtrComponents");

    let mut core = None;
    for krate in ctx.crates(()) {
        let name = ctx.crate_name(*krate);
        if name.as_str() == "core" {
            core = Some(krate);
            break;
        }
    }
    let core = if let Some(core) = core {
        *core
    } else {
        // If no crates, assume we are compiling core.
        if ctx.crates(()).is_empty() {
            use rustc_span::def_id::CrateNum;
            CrateNum::from_u32(0)
        } else {
            panic!("Could not find core. Crates:{:?}", ctx.crates(()));
        }
    };
    let core_symbols = ctx.exported_symbols(core);
    let mut max_index = 0;
    for symbol in core_symbols {
        match symbol.0 {
            ExportedSymbol::ThreadLocalShim(def_id)
            | ExportedSymbol::Generic(def_id, _)
            | ExportedSymbol::NonGeneric(def_id) => {
                max_index = max_index.max(def_id.index.as_u32());
            }
            _ => (),
        }
    }
    let mut ptr_components = None;
    for index in 0..max_index {
        let did = DefId {
            index: index.into(),
            krate: core,
        };
        let name = format!("{did:?}");

        if name.contains("ptr::metadata::PtrComponents")
            && !name.contains("PtrComponents::data_pointer")
            && !name.contains("PtrComponents::metadata")
            && !name.contains("PtrComponents::T")
        {
            assert!(
                ptr_components.is_none(),
                "Found more than one defintin of PtrComponents"
            );
            ptr_components = Some(did);
            break;
        }

        //44548
    }
    drop(find_ptr_components_timer);
    ptr_components.expect("Could not find core::ptr::metadata::PtrComponents")
}
