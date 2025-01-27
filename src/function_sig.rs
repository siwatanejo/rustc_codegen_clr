use crate::{
    codegen_error::CodegenError,
    r#type::{TyCache, Type},
};
use rustc_middle::ty::{Instance, List, ParamEnv, ParamEnvAnd, PolyFnSig, Ty, TyCtxt, TyKind};
use rustc_target::abi::call::Conv;
use rustc_target::spec::abi::Abi as TargetAbi;
use serde::{Deserialize, Serialize};
/// Function signature.
#[derive(Clone, PartialEq, Serialize, Deserialize, Eq, Hash, Debug)]
pub struct FnSig {
    inputs: Vec<Type>,
    output: Type,
}
impl FnSig {
    /// Creates a `FnSig` from ``. May not match the result of `sig_from_instance_`!
    /// Use ONLY for function pointers!
    pub fn from_poly_sig<'tyctx>(
        method_instance: Option<Instance<'tyctx>>,
        tyctx: TyCtxt<'tyctx>,
        tycache: &mut TyCache,
        sig: PolyFnSig<'tyctx>,
    ) -> Self {
        let sig = if let Some(method_instance) = method_instance {
            crate::utilis::monomorphize(&method_instance, sig, tyctx)
        } else {
            sig
        };
        let sig = tyctx.normalize_erasing_late_bound_regions(ParamEnv::reveal_all(), sig);
        let output = tycache.type_from_cache(sig.output(), tyctx, method_instance);
        let inputs: Box<[Type]> = sig
            .inputs()
            .iter()
            .map(|input| tycache.type_from_cache(*input, tyctx, method_instance))
            .collect();
        FnSig::new(&inputs, &output)
    }
    /// Returns the signature of function behind `function`.
    pub fn sig_from_instance_<'tyctx>(
        function: Instance<'tyctx>,
        tyctx: TyCtxt<'tyctx>,
        tycache: &mut TyCache,
    ) -> Result<Self, CodegenError> {
        let fn_abi = tyctx.fn_abi_of_instance(ParamEnvAnd {
            param_env: ParamEnv::reveal_all(),
            value: (function, List::empty()),
        });
        let fn_abi = match fn_abi {
            Ok(abi) => abi,
            Err(_error) => todo!(),
        };
        let conv = fn_abi.conv;
        match conv {
            Conv::Rust => (),
            Conv::C => (),
            _ => panic!("ERROR:calling using convention {conv:?} is not supported!"),
        }
        //assert!(!fn_abi.c_variadic);
        let ret = crate::utilis::monomorphize(&function, fn_abi.ret.layout.ty, tyctx);
        let ret = tycache.type_from_cache(ret, tyctx, Some(function));
        let mut args = Vec::with_capacity(fn_abi.args.len());
        for arg in fn_abi.args.iter() {
            let arg = crate::utilis::monomorphize(&function, arg.layout.ty, tyctx);
            args.push(tycache.type_from_cache(arg, tyctx, Some(function)));
        }
        // There are 2 ABI enums for some reasons(they differ in what memebers they have)
        let fn_ty = function.ty(tyctx, ParamEnv::reveal_all());
        let internal_abi = match fn_ty.kind() {
            TyKind::FnDef(_, _) => fn_ty.fn_sig(tyctx),
            TyKind::Closure(_, args) => args.as_closure().sig(),
            _ => todo!("Can't get signature of {fn_ty}"),
        }
        .abi();
        // Only those ABIs are supported
        match internal_abi {
            TargetAbi::C { unwind: _ } => (),
            TargetAbi::Cdecl { unwind: _ } => (),
            TargetAbi::RustIntrinsic => (),
            TargetAbi::Rust => (),
            TargetAbi::RustCold => (),
            TargetAbi::RustCall => (), /*Err(CodegenError::FunctionABIUnsuported(
            "\"rust_call\" ABI, used for things like clsoures, is not supported yet!",
            ))?,*/
            _ => todo!("Unsuported ABI:{internal_abi:?}"),
        }
        Ok(Self {
            inputs: args,
            output: ret,
        })
    }
    /// Returns the list of function inputs.
    #[must_use]
    pub fn inputs(&self) -> &[Type] {
        &self.inputs
    }
    /// Returns the function output.
    #[must_use]
    pub fn output(&self) -> &Type {
        &self.output
    }
    /// Creates a new function signature. For non-static functions, this must include the hidden first `this` argument!
    #[must_use]
    pub fn new(inputs: &[Type], output: &Type) -> Self {
        Self {
            inputs: inputs.into(),
            output: output.clone(),
        }
    }
    /// Sets the input list of this method.
    pub fn set_inputs(&mut self, inputs: Vec<Type>) {
        self.inputs = inputs;
    }

    pub fn inputs_mut(&mut self) -> &mut Vec<Type> {
        &mut self.inputs
    }
}
/// Checks if this function is variadic.
#[must_use]
pub fn is_fn_variadic<'tyctx>(ty: Ty<'tyctx>, tyctx: TyCtxt<'tyctx>) -> bool {
    ty.fn_sig(tyctx).skip_binder().c_variadic
}
