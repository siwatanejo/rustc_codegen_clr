use crate::cil::{CallSite, FieldDescriptor};
use crate::cil_tree::cil_node::CILNode;
use crate::cil_tree::cil_root::CILRoot;
use crate::function_sig::FnSig;
use crate::operand::handle_operand;

use crate::{conv_usize, ld_field, ldc_i32, ldc_u64, size_of};

use crate::r#type::{pointer_to_is_fat, TyCache, Type};
use rustc_middle::{
    mir::{CastKind, NullOp, Place, Rvalue},
    ty::{
        adjustment::PointerCoercion, GenericArgs, Instance, InstanceDef, ParamEnv, Ty, TyCtxt,
        TyKind,
    },
};
pub fn handle_rvalue<'tcx>(
    rvalue: &Rvalue<'tcx>,
    tyctx: TyCtxt<'tcx>,
    target_location: &Place<'tcx>,
    method: &rustc_middle::mir::Body<'tcx>,
    method_instance: Instance<'tcx>,
    tycache: &mut TyCache,
) -> CILNode {
    match rvalue {
        Rvalue::Use(operand) => handle_operand(operand, tyctx, method, method_instance, tycache),
        Rvalue::CopyForDeref(place) => {
            crate::place::place_get(place, tyctx, method, method_instance, tycache)
        }
        Rvalue::Ref(_region, _kind, place) => {
            crate::place::place_adress(place, tyctx, method, method_instance, tycache)
        }
        Rvalue::AddressOf(_mutability, place) => {
            crate::place::place_adress(place, tyctx, method, method_instance, tycache)
        }
        Rvalue::Cast(
            CastKind::PointerCoercion(
                PointerCoercion::MutToConstPointer | PointerCoercion::ArrayToPointer,
            )
            | CastKind::PtrToPtr,
            operand,
            dst,
        ) => {
            let target = crate::utilis::monomorphize(&method_instance, *dst, tyctx);
            let target_pointed_to = match target.kind() {
                TyKind::RawPtr(typ, _) => *typ,
                TyKind::Ref(_, inner, _) => *inner,
                _ => panic!("Type is not ptr {target:?}."),
            };
            let source =
                crate::utilis::monomorphize(&method_instance, operand.ty(method, tyctx), tyctx);
            let source_pointed_to = match source.kind() {
                TyKind::RawPtr(typ, _) => *typ,
                TyKind::Ref(_, inner, _) => *inner,
                _ => panic!("Type is not ptr {target:?}."),
            };
            let source_type = tycache.type_from_cache(source, tyctx, Some(method_instance));
            let target_type = tycache.type_from_cache(target, tyctx, Some(method_instance));
            //let target_type = tycache.type_from_cache(target, tyctx, Some(method_instance));
            let src_fat = pointer_to_is_fat(source_pointed_to, tyctx, Some(method_instance));
            let target_fat = pointer_to_is_fat(target_pointed_to, tyctx, Some(method_instance));
            match (src_fat, target_fat) {
                (true, true) => {
                    let parrent = handle_operand(operand, tyctx, method, method_instance, tycache);

                    crate::place::deref_op(
                        crate::place::PlaceTy::Ty(target),
                        tyctx,
                        &method_instance,
                        tycache,
                        CILNode::TemporaryLocal(Box::new((
                            source_type,
                            [CILRoot::SetTMPLocal { value: parrent }].into(),
                            CILNode::LoadAddresOfTMPLocal,
                        ))),
                    )
                }
                (true, false) => {
                    if source_type.as_dotnet().is_none() {
                        eprintln!("source:{source:?}");
                    }
                    CILNode::TemporaryLocal(Box::new((
                        source_type.clone(),
                        [CILRoot::SetTMPLocal {
                            value: handle_operand(operand, tyctx, method, method_instance, tycache),
                        }]
                        .into(),
                        CILNode::TransmutePtr {
                            val: Box::new(ld_field!(
                                CILNode::LoadAddresOfTMPLocal,
                                FieldDescriptor::new(
                                    source_type.as_dotnet().unwrap(),
                                    Type::Ptr(Type::Void.into()),
                                    "data_pointer".into(),
                                )
                            )),
                            new_ptr: Box::new(target_type),
                        },
                    )))
                }
                _ => CILNode::TransmutePtr {
                    val: Box::new(handle_operand(
                        operand,
                        tyctx,
                        method,
                        method_instance,
                        tycache,
                    )),
                    new_ptr: Box::new(target_type),
                },
            }
        }
        Rvalue::Cast(CastKind::PointerCoercion(PointerCoercion::Unsize), operand, target) => {
            crate::unsize::unsize(tyctx, method, method_instance, tycache, operand, *target)
        }
        Rvalue::BinaryOp(binop, operands) => crate::binop::binop(
            *binop,
            &operands.0,
            &operands.1,
            tyctx,
            method,
            method_instance,
            tycache,
        ),
       
        Rvalue::UnaryOp(binop, operand) => {
            crate::unop::unop(*binop, operand, tyctx, method, method_instance, tycache)
        }
        Rvalue::Cast(CastKind::IntToInt, operand, target) => {
            let target = crate::utilis::monomorphize(&method_instance, *target, tyctx);
            let target = tycache.type_from_cache(target, tyctx, Some(method_instance));
            let src = operand.ty(&method.local_decls, tyctx);
            let src = crate::utilis::monomorphize(&method_instance, src, tyctx);
            let src = tycache.type_from_cache(src, tyctx, Some(method_instance));
            crate::casts::int_to_int(
                src,
                &target,
                handle_operand(operand, tyctx, method, method_instance, tycache),
            )
        }
        Rvalue::Cast(CastKind::FloatToInt, operand, target) => {
            let target = crate::utilis::monomorphize(&method_instance, *target, tyctx);
            let target = tycache.type_from_cache(target, tyctx, Some(method_instance));
            let src = operand.ty(&method.local_decls, tyctx);
            let src = crate::utilis::monomorphize(&method_instance, src, tyctx);
            let src = tycache.type_from_cache(src, tyctx, Some(method_instance));

            crate::casts::float_to_int(
                src,
                &target,
                handle_operand(operand, tyctx, method, method_instance, tycache),
            )
        }
        Rvalue::Cast(CastKind::IntToFloat, operand, target) => {
            let target = crate::utilis::monomorphize(&method_instance, *target, tyctx);
            let target = tycache.type_from_cache(target, tyctx, Some(method_instance));
            let src = operand.ty(&method.local_decls, tyctx);
            let src = crate::utilis::monomorphize(&method_instance, src, tyctx);
            let src = tycache.type_from_cache(src, tyctx, Some(method_instance));
            crate::casts::int_to_float(
                src,
                &target,
                handle_operand(operand, tyctx, method, method_instance, tycache),
            )
        }
        Rvalue::NullaryOp(op, ty) => match op {
            NullOp::SizeOf => {
                let ty = crate::utilis::monomorphize(&method_instance, *ty, tyctx);
                let ty = tycache.type_from_cache(ty, tyctx, Some(method_instance));
                conv_usize!(size_of!(ty))
            }
            NullOp::AlignOf => {
                let ty = crate::utilis::monomorphize(&method_instance, *ty, tyctx);
                conv_usize!(ldc_u64!(crate::utilis::align_of(ty, tyctx) as u64))
            }
            NullOp::OffsetOf(fields) => {
                assert_eq!(fields.len(), 1);
                //let (variant, field) = fields[0];
                todo!("Can't calc offset of yet!");
            }

            rustc_middle::mir::NullOp::UbChecks => {
                if tyctx.sess.ub_checks() {
                    CILNode::LdTrue
                } else {
                    CILNode::LdFalse
                }
            }
        },
        Rvalue::Aggregate(aggregate_kind, field_index) => crate::aggregate::handle_aggregate(
            tyctx,
            target_location,
            method,
            aggregate_kind.as_ref(),
            field_index,
            method_instance,
            tycache,
        ),
        Rvalue::Cast(CastKind::Transmute, operand, dst) => {
            let dst = crate::utilis::monomorphize(&method_instance, *dst, tyctx);
            let dst_ty = dst;
            let dst = tycache.type_from_cache(dst, tyctx, Some(method_instance));
            let src = operand.ty(&method.local_decls, tyctx);
            let src = crate::utilis::monomorphize(&method_instance, src, tyctx);
            let src = tycache.type_from_cache(src, tyctx, Some(method_instance));
            match (&src, &dst) {
                (
                    Type::ISize | Type::USize | Type::Ptr(_),
                    Type::ISize | Type::USize | Type::Ptr(_),
                ) => CILNode::TransmutePtr {
                    val: Box::new(handle_operand(
                        operand,
                        tyctx,
                        method,
                        method_instance,
                        tycache,
                    )),
                    new_ptr: Box::new(dst),
                },
                (Type::U16, Type::DotnetChar) => {
                    handle_operand(operand, tyctx, method, method_instance, tycache)
                }

                (_, Type::F64) => CILNode::TemporaryLocal(Box::new((
                    src,
                    [CILRoot::SetTMPLocal {
                        value: handle_operand(operand, tyctx, method, method_instance, tycache),
                    }]
                    .into(),
                    CILNode::LDIndF64 {
                        ptr: CILNode::LoadAddresOfTMPLocal.into(),
                    },
                ))),
                (_, _) => CILNode::TemporaryLocal(Box::new((
                    src,
                    [CILRoot::SetTMPLocal {
                        value: handle_operand(operand, tyctx, method, method_instance, tycache),
                    }]
                    .into(),
                    crate::place::deref_op(
                        crate::place::PlaceTy::Ty(dst_ty),
                        tyctx,
                        &method_instance,
                        tycache,
                        CILNode::TransmutePtr {
                            val: Box::new(CILNode::LoadAddresOfTMPLocal),
                            new_ptr: Box::new(Type::Ptr(Box::new(dst))),
                        },
                    ),
                ))),
            }
        }
        Rvalue::ShallowInitBox(operand, dst) => {
            let dst = crate::utilis::monomorphize(&method_instance, *dst, tyctx);
            let boxed_dst = Ty::new_box(tyctx, dst);
            //let dst = tycache.type_from_cache(dst, tyctx, Some(method_instance));
            let src = operand.ty(&method.local_decls, tyctx);
            let src = crate::utilis::monomorphize(&method_instance, src, tyctx);
            let src = tycache.type_from_cache(src, tyctx, Some(method_instance));
            CILNode::TemporaryLocal(Box::new((
                Type::Ptr(src.into()),
                [CILRoot::SetTMPLocal {
                    value: handle_operand(operand, tyctx, method, method_instance, tycache),
                }]
                .into(),
                crate::place::deref_op(
                    crate::place::PlaceTy::Ty(boxed_dst),
                    tyctx,
                    &method_instance,
                    tycache,
                    CILNode::LoadAddresOfTMPLocal,
                ),
            )))
        }
        Rvalue::Cast(CastKind::PointerWithExposedProvenance, operand, target) => {
            //FIXME: the documentation of this cast(https://doc.rust-lang.org/nightly/std/ptr/fn.from_exposed_addr.html) is a bit confusing,
            //since this seems to be something deeply linked to the rust memory model.
            // I assume this to be ALWAYS equivalent to `usize as *const/mut T`, but this may not always be the case.
            // If something breaks in the fututre, this is a place that needs checking.
            let target = crate::utilis::monomorphize(&method_instance, *target, tyctx);
            let target = tycache.type_from_cache(target, tyctx, Some(method_instance));
            // Cast from usize/isize to any *T is a NOP, so we just have to load the operand.
            CILNode::TransmutePtr {
                val: Box::new(handle_operand(
                    operand,
                    tyctx,
                    method,
                    method_instance,
                    tycache,
                )),
                new_ptr: Box::new(target),
            }
        }
        Rvalue::Cast(CastKind::PointerExposeProvenance, operand, target) => {
            //FIXME: the documentation of this cast(https://doc.rust-lang.org/nightly/std/primitive.pointer.html#method.expose_addrl) is a bit confusing,
            //since this seems to be something deeply linked to the rust memory model.
            // I assume this to be ALWAYS equivalent to `*const/mut T as usize`, but this may not always be the case.
            // If something breaks in the fututre, this is a place that needs checking.
            let target = crate::utilis::monomorphize(&method_instance, *target, tyctx);
            let target = tycache.type_from_cache(target, tyctx, Some(method_instance));
            // Cast to usize/isize from any *T is a NOP, so we just have to load the operand.
            CILNode::TransmutePtr {
                val: Box::new(handle_operand(
                    operand,
                    tyctx,
                    method,
                    method_instance,
                    tycache,
                )),
                new_ptr: Box::new(target),
            }
        }
        Rvalue::Cast(CastKind::FloatToFloat, operand, target) => {
            let target = crate::utilis::monomorphize(&method_instance, *target, tyctx);
            let target = tycache.type_from_cache(target, tyctx, Some(method_instance));
            let mut ops = handle_operand(operand, tyctx, method, method_instance, tycache);
            match target {
                Type::F32 => ops = CILNode::ConvF32(ops.into()),
                Type::F64 => ops = CILNode::ConvF64(ops.into()),
                _ => panic!("Can't preform a FloatToFloat cast to type {target:?}"),
            }
            ops
        }
        Rvalue::Cast(
            CastKind::PointerCoercion(PointerCoercion::ReifyFnPointer),
            operand,
            target,
        ) => {
            let operand_ty = operand.ty(method, tyctx);
            operand
                .constant()
                .expect("function must be constant in order to take its adress!");
            let operand_ty = crate::utilis::monomorphize(&method_instance, operand_ty, tyctx);
            let _target = crate::utilis::monomorphize(&method_instance, *target, tyctx);
            let (instance, _subst_ref) = if let TyKind::FnDef(def_id, subst_ref) = operand_ty.kind()
            {
                let subst = crate::utilis::monomorphize(&method_instance, *subst_ref, tyctx);
                let env = ParamEnv::reveal_all();
                let Some(instance) =
                    Instance::resolve(tyctx, env, *def_id, subst).expect("Invalid function def")
                else {
                    panic!("ERROR: Could not get function instance. fn type:{operand_ty:?}")
                };

                (instance, subst_ref)
            } else {
                todo!("Trying to call a type which is not a function definition!");
            };
            let function_name = crate::utilis::function_name(tyctx.symbol_name(instance));
            let function_sig = FnSig::sig_from_instance_(instance, tyctx, tycache)
                .expect("Could not get function signature when trying to get a function pointer!");
            //FIXME: propely handle `#[track_caller]`
            let call_site = CallSite::new(None, function_name, function_sig, true);
            CILNode::LDFtn(call_site.into())
        }
        //Rvalue::Cast(kind, _operand, _) => todo!("Unhandled cast kind {kind:?}, rvalue:{rvalue:?}"),
        Rvalue::Discriminant(place) => {
            let addr = crate::place::place_adress(place, tyctx, method, method_instance, tycache);
            let owner_ty = place.ty(method, tyctx).ty;
            let owner_ty = crate::utilis::monomorphize(&method_instance, owner_ty, tyctx);
            let owner = tycache.type_from_cache(owner_ty, tyctx, Some(method_instance));
            //TODO: chose proper tag type based on variant count of `owner`
            //let discr_ty = owner_ty.discriminant_ty(tyctx);
            //let discr_type = tycache.type_from_cache(discr_ty, tyctx, Some(method_instance));
            let layout = tyctx
                .layout_of(rustc_middle::ty::ParamEnvAnd {
                    param_env: ParamEnv::reveal_all(),
                    value: owner_ty,
                })
                .expect("Could not get type layout!");
            let (disrc_type, _) = crate::utilis::adt::enum_tag_info(layout.layout, tyctx);
            let owner = if let crate::r#type::Type::DotnetType(dotnet_type) = owner {
                dotnet_type.as_ref().clone()
            } else {
                panic!();
            };

            let target = tycache.type_from_cache(
                owner_ty.discriminant_ty(tyctx),
                tyctx,
                Some(method_instance),
            );
            if disrc_type == Type::Void {
                // Just alwways return 0 if the discriminat type is `()` - this seems to work, and be what rustc expects. Wierd, but OK.
                crate::casts::int_to_int(Type::I32, &target, ldc_i32!(0))
            } else {
                crate::casts::int_to_int(
                    disrc_type.clone(),
                    &target,
                    crate::utilis::adt::get_discr(layout.layout, addr, owner, tyctx, owner_ty),
                )
            }
        }
        Rvalue::Len(operand) => {
            let ty = operand.ty(method, tyctx);
            let ty = crate::utilis::monomorphize(&method_instance, ty, tyctx);
            // let tpe = tycache.type_from_cache(ty.ty, tyctx, Some(method_instance));
            match ty.ty.kind() {
                TyKind::Slice(inner) => {
                    let slice_tpe = tycache
                        .slice_ty(*inner, tyctx, Some(method_instance))
                        .as_dotnet()
                        .unwrap();
                    let descriptor =
                        FieldDescriptor::new(slice_tpe, Type::USize, "metadata".into());
                    let addr = crate::place::place_address_raw(
                        operand,
                        tyctx,
                        method,
                        method_instance,
                        tycache,
                    );
                    assert!(
                        !matches!(addr, CILNode::LDLoc(_)),
                        "improper addr {addr:?}. operand:{operand:?}"
                    );
                    ld_field!(addr, descriptor)
                }
                _ => todo!("Get length of type {ty:?}"),
            }
        }
        Rvalue::Repeat(operand, times) => {
            let times = crate::utilis::monomorphize(&method_instance, *times, tyctx);
            let times = times
                .try_eval_target_usize(tyctx, ParamEnv::reveal_all())
                .expect("Could not evalute array size as usize.");
            let array =
                crate::utilis::monomorphize(&method_instance, rvalue.ty(method, tyctx), tyctx);
            let array = tycache.type_from_cache(array, tyctx, Some(method_instance));
            let array_dotnet = array.clone().as_dotnet().expect("Invalid array type.");

            let operand_type = tycache.type_from_cache(
                crate::utilis::monomorphize(&method_instance, operand.ty(method, tyctx), tyctx),
                tyctx,
                Some(method_instance),
            );
            let operand = handle_operand(operand, tyctx, method, method_instance, tycache);
            let mut branches = Vec::new();
            for idx in 0..times {
                branches.push(CILRoot::Call {
                    site: CallSite::new(
                        Some(array_dotnet.clone()),
                        "set_Item".into(),
                        FnSig::new(
                            &[
                                Type::Ptr(array.clone().into()),
                                Type::USize,
                                operand_type.clone(),
                            ],
                            &Type::Void,
                        ),
                        false,
                    ),
                    args: [
                        CILNode::LoadAddresOfTMPLocal,
                        conv_usize!(ldc_u64!(idx)),
                        operand.clone(),
                    ]
                    .into(),
                });
            }
            let branches: Box<_> = branches.into();
            CILNode::TemporaryLocal(Box::new((array.clone(), branches, CILNode::LoadTMPLocal)))
        }
        Rvalue::ThreadLocalRef(def_id) => {
            if !def_id.is_local() && tyctx.needs_thread_local_shim(*def_id) {
                let _instance = Instance {
                    def: InstanceDef::ThreadLocalShim(*def_id),
                    args: GenericArgs::empty(),
                };
                // Call instance
                todo!("Thread locals with shims unsupported!")
            } else {
                let alloc_id = tyctx.reserve_and_set_static_alloc(*def_id);
                CILNode::LoadGlobalAllocPtr {
                    alloc_id: alloc_id.0.into(),
                }
            }
        }
        Rvalue::Cast(rustc_middle::mir::CastKind::FnPtrToPtr, _, _) => {
            todo!("Unusported cast kind:FnPtrToPtr")
        }
        Rvalue::Cast(rustc_middle::mir::CastKind::DynStar, _, _) => {
            todo!("Unusported cast kind:DynStar")
        }
        Rvalue::Cast(_, _, _) => todo!(),
    }
}
