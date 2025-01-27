use super::AssemblyExporter;
use crate::cil_tree::cil_root::CILRoot;

use crate::r#type::TypeDef;
use crate::{
    cil_tree::{cil_node::CILNode, CILTree},
    method::Method,
    r#type::Type,
    IString,
};
use std::collections::HashMap;
use std::hash::Hasher;
use std::process::Command;
use std::{borrow::Cow, collections::HashSet, io::Write};
pub struct CExporter {
    types: Vec<u8>,
    type_defs: Vec<u8>,
    method_defs: Vec<u8>,
    static_defs: Vec<u8>,
    encoded_asm: Vec<u8>,
    headers: Vec<u8>,
    defined: HashSet<IString>,
    delayed_typedefs: HashMap<IString, TypeDef>,
}
impl std::io::Write for CExporter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.encoded_asm.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.encoded_asm.flush()
    }
}
fn escape_type_name(name: &str) -> String {
    name.replace(['.', ' '], "_")
        .replace('<', "lt")
        .replace('>', "gt")
        .replace('$', "ds")
        .replace(',', "cm")
        .replace('{', "bs")
        .replace('}', "be")
        .replace('+', "ps")
}
impl CExporter {
    fn as_source(&self, is_dll: bool) -> Vec<u8> {
        let mut res = self.headers.clone();
        res.extend(&self.types);
        res.extend(&self.type_defs);
        res.extend(&self.method_defs);
        res.extend(&self.static_defs);
        res.extend(&self.encoded_asm);
        if !is_dll {
            writeln!(res, "int main(int argc,char** argv){{_cctor();exec_fname = argv[0];entrypoint(argv + 1);}}").unwrap();
        }
        res
    }
    fn add_method_inner(&mut self, method: &crate::method::Method, class: Option<&str>) {
        //eprintln!("C source:\n{}",String::from_utf8_lossy(&self.as_source()));
        let sig = method.sig();

        let name = method.name().replace('.', "_");
        // Puts is already defined in C.
        if name == "puts"
            || name == "malloc"
            || name == "printf"
            || name == "free"
            || name == "realloc"
            || name == "syscall"
        {
            return;
        }
        let output = c_tpe(sig.output());
        let mut inputs: String = "(".into();
        let mut input_iter = sig
            .inputs()
            .iter()
            .enumerate()
            .filter(|(_, tpe)| **tpe != Type::Void);
        if let Some((idx, input)) = input_iter.next() {
            inputs.push_str(&format!("{input} A{idx}", input = c_tpe(input)));
        }
        for (idx, input) in input_iter {
            inputs.push_str(&format!(",{input} A{idx} ", input = c_tpe(input)));
        }
        inputs.push(')');
        let mut code = String::new();
        for (id, (_, local)) in method.locals().iter().enumerate() {
            if *local == Type::Void {
                continue;
            }
            code.push_str(&format!("\t{local} L{id};\n", local = c_tpe(local)));
        }
        for bb in method.blocks() {
            code.push_str(&format!("\tBB_{}:\n", bb.id()));
            for tree in bb.trees() {
                code.push_str(&format!("{}\n", tree_string(tree, method)));
                //code.push_str(&format!("/*{tree:?}*/\n"));
            }
        }
        if let Some(class) = class {
            let class = escape_type_name(class);
            writeln!(self.method_defs, "{output} {class}{name} {inputs};").unwrap();
            write!(
                self.encoded_asm,
                "{output} {class}{name} {inputs}{{\n{code}}}\n"
            )
            .unwrap();
        } else {
            writeln!(self.method_defs, "{output} {name} {inputs};").unwrap();
            write!(self.encoded_asm, "{output} {name} {inputs}{{\n{code}}}\n").unwrap();
        }
    }
}
impl AssemblyExporter for CExporter {
    fn init(_asm_info: &super::AssemblyInfo) -> Self {
        let mut encoded_asm = Vec::with_capacity(0x1_00);
        let types = Vec::with_capacity(0x1_00);
        let type_defs = Vec::with_capacity(0x1_00);
        let method_defs = Vec::with_capacity(0x1_00);
        let static_defs = Vec::with_capacity(0x1_00);
        let mut headers = Vec::with_capacity(0x1_00);
        write!(headers, "/*  This file was autogenerated by `rustc_codegen_clr` by FractalFir\n It contains C code made from Rust.*/\n").expect("Write error!");

        write!(
            headers,
            "#include  <stdint.h>\n#include <stdbool.h>\n#include <stddef.h>\n#include <stdio.h>\n#include <stdlib.h>\n#include <mm_malloc.h>\n#include <sys/syscall.h>\n"
        )
        .expect("Write error!");
        headers.write_all(include_bytes!("c_header.h")).unwrap();
        writeln!(headers).expect("Write error!");
        writeln!(
            encoded_asm,
            "#pragma GCC diagnostic ignored \"-Wmaybe-uninitialized\""
        )
        .unwrap();
        writeln!(
            encoded_asm,
            "#pragma GCC diagnostic ignored \"-Wunused-label\""
        )
        .unwrap();
        writeln!(
            encoded_asm,
            "#pragma GCC diagnostic ignored \"-Wunused-but-set-variable\""
        )
        .unwrap();
        writeln!(
            encoded_asm,
            "#pragma GCC diagnostic ignored \"-Wunused-variable\""
        )
        .unwrap();
        writeln!(
            encoded_asm,
            "#pragma GCC diagnostic ignored \"-Wpointer-sign\""
        )
        .unwrap();
        Self {
            types,
            type_defs,
            encoded_asm,
            method_defs,
            static_defs,
            headers,
            defined: HashSet::new(),
            delayed_typedefs: HashMap::new(),
        }
    }
    fn add_type(&mut self, tpe: &crate::r#type::TypeDef) {
        let name: IString = escape_type_name(tpe.name()).into();
        if self.defined.contains(&name) {
            return;
        }
        for tpe_name in tpe
            .fields()
            .iter()
            .filter_map(|field| field.1.as_dotnet())
            .filter_map(|tpe| {
                if tpe.asm().is_none() {
                    Some(escape_type_name(tpe.name_path()))
                } else {
                    None
                }
            })
        {
            if !self.defined.contains::<Box<_>>(&tpe_name.clone().into()) {
                //eprintln!("type {tpe_name:?} has unresolved dependencies");
                self.delayed_typedefs.insert(name, tpe.clone());
                return;
            }
        }
        let mut fields = String::new();
        if let Some(offsets) = tpe.explicit_offsets() {
            for ((field_name, field_type), offset) in tpe.fields().iter().zip(offsets) {
                if *field_type == Type::Void {
                    continue;
                }

                fields.push_str(&format!(
                    "\tstruct {{char pad[{offset}];{field_type} f;}} {field_name};\n\n",
                    field_type = c_tpe(field_type)
                ));
            }
        } else {
            for (field_name, field_type) in tpe.fields() {
                if *field_type == Type::Void {
                    continue;
                }
                fields.push_str(&format!(
                    "\tstruct {{{field_type} f;}} {field_name};\n",
                    field_type = c_tpe(field_type)
                ));
            }
        }
        for method in tpe.methods() {
            self.add_method_inner(method, Some(&name));
        }
        if tpe.explicit_offsets().is_some() {
            writeln!(self.types, "typedef union {name} {name};").unwrap();
            write!(self.type_defs, "union {name}{{\n{fields}}};\n").unwrap();
        } else {
            writeln!(self.types, "typedef struct {name} {name};").unwrap();
            write!(self.type_defs, "struct {name}{{\n{fields}}};\n").unwrap();
        }
        self.defined.insert(name);
        let delayed_typedefs = self.delayed_typedefs.clone();
        self.delayed_typedefs = HashMap::new();
        for (_, tpe) in delayed_typedefs {
            self.add_type(&tpe);
        }
    }

    fn add_method(&mut self, method: &crate::method::Method) {
        self.add_method_inner(method, None);
    }

    fn add_extern_method(&mut self, _lib_path: &str, name: &str, sig: &crate::function_sig::FnSig) {
        if name == "puts"
            || name == "malloc"
            || name == "printf"
            || name == "free"
            || name == "syscall"
            || name == "getenv"
            || name == "rename"
        {
            return;
        }
        let output = c_tpe(sig.output());
        let mut inputs: String = "(".into();
        let mut input_iter = sig
            .inputs()
            .iter()
            .enumerate()
            .filter(|(_, tpe)| **tpe != Type::Void);
        if let Some((idx, input)) = input_iter.next() {
            inputs.push_str(&format!("{input} A{idx}", input = c_tpe(input)));
        }
        for (idx, input) in input_iter {
            inputs.push_str(&format!(",{input} A{idx} ", input = c_tpe(input)));
        }
        inputs.push(')');
        writeln!(self.method_defs, "extern {output} {name} {inputs};").unwrap();
    }

    fn finalize(
        self,
        final_path: &std::path::Path,
        is_dll: bool,
    ) -> Result<(), super::AssemblyExportError> {
        let cc = "gcc";
        let src_path = final_path.with_extension("c");
        std::fs::File::create(&src_path)
            .unwrap()
            .write_all(&self.as_source(is_dll))
            .unwrap();
        let sanitize = if *crate::config::C_SANITIZE {
            "-fsanitize=undefined"
        } else {
            "-O"
        };
        let out = Command::new(cc)
            .args([
                "-g",
                sanitize,
                "-o",
                final_path.to_string_lossy().as_ref(),
                src_path.to_string_lossy().as_ref(),
                "-lm",
                "-fno-strict-aliasing",
            ])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!stderr.contains("error"), "C compiler error:{stderr:?}!");
        Ok(())
    }

    fn add_extern_ref(&mut self, _asm_name: &str, _info: &crate::assembly::AssemblyExternRef) {
        // Not needed in C
    }

    fn add_global(&mut self, tpe: &crate::r#type::Type, name: &str) {
        writeln!(self.static_defs, "static {tpe} {name};", tpe = c_tpe(tpe)).unwrap();
    }
}
fn node_string(tree: &CILNode, method: &Method) -> String {
    match tree {
        CILNode::LocAllocAligned { tpe, align }=>format!("(((uintptr_t)alloca(sizeof({tpe})) + {align})) + ({align} - 1)) & (-{align}))",tpe = c_tpe(tpe)),
        CILNode::PointerToConstValue(_value) => {
            panic!("ERROR: const values must be allocated before CIL export phase.")
        }
        CILNode::LDLoc(loc) => format!("L{loc}"),
        CILNode::LDArg(arg) => format!("A{arg}"),
        CILNode::LDLocA(arg) => format!("((uintptr_t)(void*)&L{arg})"),
        CILNode::LDArgA(loc) => format!("((uintptr_t)(void*)&A{loc})"),
        CILNode::BlackBox(inner) => format!("black_box({val})", val = node_string(inner, method)),
        CILNode::LDStaticField(static_field) => static_field.name().into(),
        CILNode::ConvF32(inner) => format!("((float){inner})", inner = node_string(inner, method)),
        CILNode::ConvF64(inner) | CILNode::ConvF64Un(inner) => {
            format!("((double){inner})", inner = node_string(inner, method))
        }
        CILNode::SizeOf(tpe) => format!("sizeof({tpe})", tpe = c_tpe(tpe)),
        CILNode::LDIndI8 { ptr } => format!("(*((int8_t*){ptr}))", ptr = node_string(ptr, method)),
        CILNode::LDIndBool { ptr } => format!("(*((bool*){ptr}))", ptr = node_string(ptr, method)),
        CILNode::LDIndI16 { ptr } => {
            format!("(*((int16_t*){ptr}))", ptr = node_string(ptr, method))
        }
        CILNode::LDIndI32 { ptr } => {
            format!("(*((int32_t*){ptr}))", ptr = node_string(ptr, method))
        }
        CILNode::LDIndI64 { ptr } => {
            format!("(*((int64_t*){ptr}))", ptr = node_string(ptr, method))
        }
        CILNode::LDIndU8 { ptr } => format!("(*((uint8_t*){ptr}))", ptr = node_string(ptr, method)),
        CILNode::LDIndU16 { ptr } => {
            format!("(*((uint16_t*){ptr}))", ptr = node_string(ptr, method))
        }
        CILNode::LDIndU32 { ptr } => {
            format!("(*((uint32_t*){ptr}))", ptr = node_string(ptr, method))
        }
        CILNode::LDIndU64 { ptr } => {
            format!("(*((uint64_t*){ptr}))", ptr = node_string(ptr, method))
        }
        CILNode::LDIndISize { ptr } => {
            format!("(*((size_t*){ptr}))", ptr = node_string(ptr, method))
        }
        CILNode::LDIndPtr {
            ptr,
            loaded_ptr: loaded_points_to,
        } => {
            format!(
                "(*(({loaded_points_to}){ptr}))",
                ptr = node_string(ptr, method),
                loaded_points_to = c_tpe(loaded_points_to)
            )
        }
        CILNode::LDIndUSize { ptr } => {
            format!("(*((ptrdiff_t*){ptr}))", ptr = node_string(ptr, method))
        }
        CILNode::LdObj { ptr, obj } => format!(
            "(*({owner}*)({ptr}))",
            ptr = node_string(ptr, method),
            owner = c_tpe(obj)
        ),
        CILNode::LDIndF32 { ptr } => format!("(*((float*){ptr}))", ptr = node_string(ptr, method)),
        CILNode::LDIndF64 { ptr } => format!("(*((double*){ptr}))", ptr = node_string(ptr, method)),
        CILNode::LDFieldAdress { addr, field } => {
            if let CILNode::LDLoc(loc) = addr.as_ref() {
                if !matches!(
                    method.locals()[*loc as usize].1,
                    Type::Ptr(_) | Type::ISize | Type::USize
                ) {
                    return format!(
                        "(&(({owner}*)&{ptr})->{name}.f)",
                        ptr = node_string(addr, method),
                        owner = c_tpe(&field.owner().clone().into()),
                        name = field.name()
                    );
                }
            }
            if let CILNode::LDArg(arg) = addr.as_ref() {
                if !matches!(
                    method.sig().inputs()[*arg as usize],
                    Type::Ptr(_) | Type::ISize | Type::USize
                ) {
                    return format!(
                        "(&(({owner}*)&{ptr})->{name}.f)",
                        ptr = node_string(addr, method),
                        owner = c_tpe(&field.owner().clone().into()),
                        name = field.name()
                    );
                }
            }
            format!(
                "(&(({owner}*){ptr})->{name}.f)",
                ptr = node_string(addr, method),
                owner = c_tpe(&field.owner().clone().into()),
                name = field.name()
            )
        }
        CILNode::LDField { addr, field } => {
            if let CILNode::LDLoc(loc) = addr.as_ref() {
                if !matches!(
                    method.locals()[*loc as usize].1,
                    Type::Ptr(_) | Type::ISize | Type::USize
                ) {
                    return format!(
                        "//{addr:?}\n(({owner}*)&{ptr})->{name}.f",
                        ptr = node_string(addr, method),
                        owner = c_tpe(&field.owner().clone().into()),
                        name = field.name()
                    );
                }
            }
            if let CILNode::LDArg(arg) = addr.as_ref() {
                if !matches!(
                    method.sig().inputs()[*arg as usize],
                    Type::Ptr(_) | Type::ISize | Type::USize
                ) {
                    return format!(
                        "//{addr:?}\n(({owner}*)&{ptr})->{name}.f",
                        ptr = node_string(addr, method),
                        owner = c_tpe(&field.owner().clone().into()),
                        name = field.name()
                    );
                }
            }
            format!(
                "//{addr:?}\n(({owner}*){ptr})->{name}.f",
                ptr = node_string(addr, method),
                owner = c_tpe(&field.owner().clone().into()),
                name = field.name()
            )
        }
        CILNode::Add(a, b) => format!(
            "({a}) + ({b})",
            a = node_string(a, method),
            b = node_string(b, method)
        ),
        CILNode::And(a, b) => format!(
            "({a}) & ({b})",
            a = node_string(a, method),
            b = node_string(b, method)
        ),
        CILNode::Sub(a, b) => format!(
            "({a}) - ({b})",
            a = node_string(a, method),
            b = node_string(b, method)
        ),
        CILNode::Mul(a, b) => format!(
            "({a}) * ({b})",
            a = node_string(a, method),
            b = node_string(b, method)
        ),
        CILNode::Div(a, b) | CILNode::DivUn(a, b) => format!(
            "({a}) / ({b})",
            a = node_string(a, method),
            b = node_string(b, method)
        ),
        CILNode::Rem(a, b) | CILNode::RemUn(a, b) => {
            format!(
                "{a} % {b}",
                a = node_string(a, method),
                b = node_string(b, method)
            )
        }
        CILNode::Or(a, b) => format!(
            "({a}) | ({b})",
            a = node_string(a, method),
            b = node_string(b, method)
        ),
        CILNode::XOr(a, b) => format!(
            "({a}) ^ ({b})",
            a = node_string(a, method),
            b = node_string(b, method)
        ),
        CILNode::Shr(a, b) => format!(
            "{a} >> {b}",
            a = node_string(a, method),
            b = node_string(b, method)
        ),
        CILNode::Shl(a, b) | CILNode::ShrUn(a, b) => {
            format!(
                "{a} << {b}",
                a = node_string(a, method),
                b = node_string(b, method)
            )
        }
        CILNode::RawOpsParrentless { .. } => todo!(),
        CILNode::Call { args, site } | CILNode::CallVirt { args, site } => {
            let name = site.name();
            let mut input_iter = args
                .iter()
                .zip(site.signature().inputs())
                .filter_map(|(code, tpe)| if *tpe == Type::Void { Some(code) } else { None });
            let mut inputs: String = "(".into();
            if let Some(input) = input_iter.next() {
                inputs.push_str(&node_string(input, method).to_string());
            }
            for input in input_iter {
                inputs.push_str(&format!(",{input} ", input = node_string(input, method)));
            }
            inputs.push(')');
            let tpe_name = site
                .class()
                .map_or(String::new(), |tpe| escape_type_name(tpe.name_path()));
            format!("{tpe_name}{name}{inputs}")
        }
        //CILNode::CallVirt { .. } => panic!("Virtual calls not supported in C."),
        CILNode::LdcI64(value) => format!("{value}l"),
        CILNode::LdcU64(value) => format!("{value}ul"),
        CILNode::LdcI32(value) => format!("{value}"),
        CILNode::LdcU32(value) => format!("{value}u"),
        CILNode::LdcF64(value) => format!("{value}"),
        CILNode::LdcF32(value) => format!("{value}"),
        CILNode::LoadGlobalAllocPtr { .. } => todo!(),
        CILNode::ConvU8(inner) => format!("((uint8_t){inner})", inner = node_string(inner, method)),
        CILNode::ConvU16(inner) => {
            format!("((uint16_t){inner})", inner = node_string(inner, method))
        }
        CILNode::ConvU32(inner) => {
            format!("((uint32_t){inner})", inner = node_string(inner, method))
        }
        CILNode::ConvU64(inner) => {
            format!("((uint64_t){inner})", inner = node_string(inner, method))
        }
        CILNode::ZeroExtendToUSize(inner) => {
            format!("((uintptr_t){inner})", inner = node_string(inner, method))
        }
        CILNode::ZeroExtendToISize(inner) => {
            format!(
                "((intptr_t)((uintptr_t){inner}))",
                inner = node_string(inner, method)
            )
        }
        CILNode::MRefToRawPtr(inner) => node_string(inner, method),
        CILNode::ConvI8(inner) => format!("((int8_t){inner})", inner = node_string(inner, method)),
        CILNode::ConvI16(inner) => {
            format!("((int16_t){inner})", inner = node_string(inner, method))
        }
        CILNode::ConvI32(inner) => {
            format!("((int32_t){inner})", inner = node_string(inner, method))
        }
        CILNode::ConvI64(inner) => {
            format!("((int64_t){inner})", inner = node_string(inner, method))
        }
        CILNode::ConvISize(inner) => {
            format!("((ptrdiff_t){inner})", inner = node_string(inner, method))
        }
        CILNode::Neg(a) => format!("-({a})", a = node_string(a, method)),
        CILNode::Not(a) => format!("!({a})", a = node_string(a, method)),
        CILNode::Eq(a, b) => format!(
            "(({a}) == ({b}))",
            a = node_string(a, method),
            b = node_string(b, method)
        ),
        CILNode::Lt(a, b) | CILNode::LtUn(a, b) => {
            format!(
                "{a} < {b}",
                a = node_string(a, method),
                b = node_string(b, method)
            )
        }
        CILNode::Gt(a, b) | CILNode::GtUn(a, b) => {
            format!(
                "{a} > {b}",
                a = node_string(a, method),
                b = node_string(b, method)
            )
        }
        CILNode::TemporaryLocal(_) => todo!(),
        CILNode::SubTrees(sub, main) => {
            assert!(sub.is_empty(), "A sub-tree still remains!");
            println!(
                "WARNING: Sub-trees impropely resolved: an empty sub-tree list still remains!"
            );
            node_string(main, method)
        }
        CILNode::LoadAddresOfTMPLocal => todo!(),
        CILNode::LoadTMPLocal => todo!(),
        CILNode::LDFtn(fn_sig) => {
            let name = fn_sig.name();
            let tpe_name = fn_sig
                .class()
                .map_or(String::new(), |tpe| escape_type_name(tpe.name_path()));
            format!("(uintptr_t)(&{tpe_name}{name})")
        }
        CILNode::LDTypeToken(tpe) => {
            use std::hash::Hash;
            let mut hasher = std::hash::DefaultHasher::new();
            tpe.hash(&mut hasher);
            let hsh = hasher.finish();
            format!("{hsh}")
        }
        CILNode::NewObj { site, args } => {
            let mut input_iter = args
                .iter()
                .zip(site.signature().inputs())
                .filter_map(|(code, tpe)| if *tpe == Type::Void { None } else { Some(code) });
            let mut inputs: String = "(".into();
            if let Some(input) = input_iter.next() {
                inputs.push_str(&node_string(input, method).to_string());
            }
            for input in input_iter {
                inputs.push_str(&format!(",{input} ", input = node_string(input, method)));
            }
            inputs.push(')');
            let tpe_name = escape_type_name(site.class().unwrap().name_path());
            format!("ctor_{tpe_name}{inputs}")
        }
        CILNode::LdStr(string) => format!("{string:?}"),
        CILNode::CallI(_sig_ptr_args) => todo!(),
        CILNode::LDLen { arr } => todo!("arr:{arr:?}"),
        CILNode::LDElelemRef { arr, idx } => todo!("arr:{arr:?} idx:{idx:?}"),
        CILNode::GetStackTop => todo!(),
        CILNode::InspectValue { val, inspect: _ } => node_string(val, method),
        CILNode::TransmutePtr { val, new_ptr } => format!(
            "({new_ptr}){val}",
            new_ptr = c_tpe(new_ptr),
            val = node_string(val, method)
        ),
        CILNode::LdFalse => "false".into(),
        CILNode::LdTrue => "true".into(),
    }
}
fn tree_string(tree: &CILTree, method: &Method) -> String {
    match tree.root() {
        CILRoot::SourceFileInfo(sfi) => format!(
            "//{fname}:{line}:{col}",
            line = sfi.0.start,
            col = sfi.1.start,
            fname = sfi.2
        ),
        CILRoot::STLoc { local, tree } => {
            let local_ty = &method.locals()[*local as usize].1;
            if local_ty.as_dotnet().is_some() {
                format!("\tL{local} = {tree};\n", tree = node_string(tree, method))
            } else {
                format!(
                    "\tL{local} = (({local_ty}){tree});\n",
                    tree = node_string(tree, method),
                    local_ty = c_tpe(local_ty)
                )
            }
        }
        CILRoot::BTrue {
            target,
            sub_target,
            cond: ops,
        } => {
            if *sub_target != 0 {
                format!(
                    "\tif(({ops}) != 0)goto BB_{sub_target};\n",
                    ops = node_string(ops, method)
                )
            } else {
                format!(
                    "\tif(({ops}) != 0)goto BB_{target};\n",
                    ops = node_string(ops, method)
                )
            }
        }
        CILRoot::GoTo { target, sub_target } => {
            if *sub_target != 0 {
                format!("goto BB_{sub_target};")
            } else {
                format!("goto BB_{target};")
            }
        }
        CILRoot::Call { site, args } => {
            let name = site.name();
            let mut input_iter = args
                .iter()
                .zip(site.signature().inputs())
                .filter(|(_, tpe)| **tpe != Type::Void);
            let mut inputs: String = "(".into();
            if let Some((input, arg)) = input_iter.next() {
                if arg.as_dotnet().is_some() {
                    inputs.push_str(&node_string(input, method).to_string());
                } else {
                    inputs.push_str(&format!(
                        "({arg})({ops})",
                        ops = node_string(input, method),
                        arg = c_tpe(arg)
                    ));
                }
                //                inputs.push_str(&format!("{input}", input = node_string(input)));
            }
            for (input, arg) in input_iter {
                if arg.as_dotnet().is_some() {
                    // Can't cast to a struct in C.
                    inputs.push_str(&format!(",{ops}", ops = node_string(input, method)));
                } else {
                    inputs.push_str(&format!(
                        ",({arg})({ops})",
                        ops = node_string(input, method),
                        arg = c_tpe(arg)
                    ));
                }
            }
            inputs.push(')');
            let tpe_name = site
                .class()
                .map_or(String::new(), |tpe| escape_type_name(tpe.name_path()));
            format!("{tpe_name}{name}{inputs};")
        }
        CILRoot::SetField { addr, value, desc } => {
            if desc.tpe().as_dotnet().is_some() {
                format!(
                    "(({owner}*){ptr})->{name}.f = {value};",
                    ptr = node_string(addr, method),
                    owner = c_tpe(&desc.owner().clone().into()),
                    name = desc.name(),
                    value = node_string(value, method)
                )
            } else {
                format!(
                    "(({owner}*){ptr})->{name}.f = ({tpe}){value};",
                    ptr = node_string(addr, method),
                    owner = c_tpe(&desc.owner().clone().into()),
                    name = desc.name(),
                    value = node_string(value, method),
                    tpe = c_tpe(desc.tpe()),
                )
            }
        }

        CILRoot::SetTMPLocal { value } => {
            panic!("Temporary locals must be resolved before the export stage! value:{value:?}")
        }
        CILRoot::CpBlk { src, dst, len } => format!(
            "memcpy({src},{dst},{len});",
            src = node_string(src, method),
            dst = node_string(dst, method),
            len = node_string(len, method)
        ),
        CILRoot::STIndI8(addr_calc, value_calc) => format!(
            "*((int8_t*)({addr_calc})) = (int8_t){value_calc};",
            addr_calc = node_string(addr_calc, method),
            value_calc = node_string(value_calc, method)
        ),
        CILRoot::STIndI16(addr_calc, value_calc) => format!(
            "*((int16_t*)({addr_calc})) = (int16_t){value_calc};",
            addr_calc = node_string(addr_calc, method),
            value_calc = node_string(value_calc, method)
        ),
        CILRoot::STIndI32(addr_calc, value_calc) => format!(
            "*((int32_t*)({addr_calc})) = (int32_t){value_calc};",
            addr_calc = node_string(addr_calc, method),
            value_calc = node_string(value_calc, method)
        ),
        CILRoot::STIndI64(addr_calc, value_calc) => format!(
            "*((int64_t*)({addr_calc})) = (int64_t){value_calc};",
            addr_calc = node_string(addr_calc, method),
            value_calc = node_string(value_calc, method)
        ),
        CILRoot::STIndISize(addr_calc, value_calc) => format!(
            "*((uintptr_t*)({addr_calc})) = (uintptr_t){value_calc};",
            addr_calc = node_string(addr_calc, method),
            value_calc = node_string(value_calc, method)
        ),
        CILRoot::STIndF64(_, _) => todo!(),
        CILRoot::STIndF32(_, _) => todo!(),
        CILRoot::STObj {
            tpe,
            addr_calc,
            value_calc,
        } => {
            let local_ty = tpe;
            if local_ty.as_dotnet().is_some() {
                format!(
                    "*(({local_ty}*)({addr_calc})) = {value_calc};",
                    addr_calc = node_string(addr_calc, method),
                    value_calc = node_string(value_calc, method),
                    local_ty = c_tpe(local_ty)
                )
            } else {
                format!(
                    "*(({local_ty}*)({addr_calc})) = (({local_ty}){value_calc});",
                    addr_calc = node_string(addr_calc, method),
                    value_calc = node_string(value_calc, method),
                    local_ty = c_tpe(local_ty)
                )
            }
        }
        CILRoot::STArg { arg, tree } => {
            let arg_ty = &method.sig().inputs()[*arg as usize];
            if arg_ty.as_dotnet().is_some() {
                format!("\tA{arg} = {tree};\n", tree = node_string(tree, method))
            } else {
                format!(
                    "\tA{arg} = (({local_ty}){tree});\n",
                    tree = node_string(tree, method),
                    local_ty = c_tpe(arg_ty)
                )
            }
        }
        CILRoot::Break => "__debug_break()".into(),
        CILRoot::Nop => String::new(),
        CILRoot::InitBlk { dst, val, count } => {
            format!(
                "memset((void*)({dst}),({val}),(size_t)({count}));",
                dst = node_string(dst, method),
                val = node_string(val, method),
                count = node_string(count, method)
            )
        }
        CILRoot::CallVirt { .. } => panic!("Virtual calls not supported in C."),
        CILRoot::Ret { tree } => {
            if method.sig().output().as_dotnet().is_some() {
                format!("\treturn {ops};", ops = node_string(tree, method))
            } else {
                format!(
                    "\treturn ({ret}){ops};",
                    ops = node_string(tree, method),
                    ret = c_tpe(method.sig().output())
                )
            }
        }
        CILRoot::Pop { tree } => {
            format!("\t{ops};", ops = node_string(tree, method))
        }
        CILRoot::VoidRet => "return;".into(),
        CILRoot::Throw(_) => "abort();".to_string(),
        CILRoot::ReThrow => todo!(),
        CILRoot::CallI { sig, fn_ptr, args } => todo!(
            "Can't yet call function pointers in C. fn_ptr:{fn_ptr:?} sig:{sig:?} args:{args:?}"
        ),
        CILRoot::JumpingPad { ops } => {
            println!("WARNING: There should be no jumping pads in C, jet a jumping pad remains! ops:{ops:?}");
            "/*Invalid jump pad was here*/abort();\n".into()
        }
        CILRoot::SetStaticField { descr, value } => {
            let local_ty = descr.tpe();
            if local_ty.as_dotnet().is_some() {
                format!(
                    "{name} = {value_calc};",
                    name = descr.name(),
                    value_calc = node_string(value, method)
                )
            } else {
                format!(
                    "{name} = (({local_ty}){value_calc});",
                    name = descr.name(),
                    value_calc = node_string(value, method),
                    local_ty = c_tpe(local_ty)
                )
            }
        }
    }
}
fn c_tpe(tpe: &Type) -> Cow<'static, str> {
    match tpe {
        Type::Bool => "bool".into(),
        Type::USize => "uintptr_t".into(),
        Type::ISize => "ptrdiff_t".into(),
        Type::Void => "void".into(),
        Type::DotnetChar => "char".into(),
        Type::I128 => "__int128".into(),
        Type::U128 => "unsigned __int128".into(),
        Type::I64 => "int64_t".into(),
        Type::U64 => "uint64_t".into(),
        Type::I32 => "int32_t".into(),
        Type::U32 => "uint32_t".into(),
        Type::F64 => "float".into(),
        Type::F32 => "double".into(),
        Type::I16 => "int16_t".into(),
        Type::U16 => "uint16_t".into(),
        Type::I8 => "int8_t".into(),
        Type::U8 => "uint8_t".into(),
        Type::Ptr(inner) => format!("{inner}*", inner = c_tpe(inner)).into(),
        Type::DotnetType(tref) => {
            if let Some(asm) = tref.asm() {
                match (asm, tref.name_path()) {
                    ("System.Runtime", "System.UInt128") => return c_tpe(&Type::U128),
                    ("System.Runtime", "System.Int128") => return c_tpe(&Type::I128),
                    _ => println!("Type {tref:?} is not supported in C"),
                }
            }
            // Ugly hack to deal with `c_void`
            if tref.name_path().contains("c_void")
                && tref.name_path().contains("ffi")
                && tref.name_path().contains("core")
            {
                return c_tpe(&Type::Void);
            }
            escape_type_name(tref.name_path()).into()
        }
        Type::DelegatePtr(_sig) => "void*".into(),
        Type::ManagedArray { element, dims } => {
            let ptrs: String = (0..(dims.get())).map(|_| '*').collect();
            format!("{element}{ptrs}", element = c_tpe(element)).into()
        }
        _ => todo!("Unsuported type {tpe:?}"),
    }
}
