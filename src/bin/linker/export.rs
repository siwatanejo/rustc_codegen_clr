use std::path::Path;

use rustc_codegen_clr::{
    assembly::Assembly,
    assembly_exporter::{AssemblyExportError, AssemblyExporter},
};

pub fn export_assembly(
    asm: &Assembly,
    path: impl AsRef<Path>,
    is_lib: bool,
) -> Result<(), AssemblyExportError> {
    rustc_codegen_clr::assembly_exporter::ilasm_exporter::ILASMExporter::export_assembly(
        asm,
        path.as_ref(),
        is_lib,
    )
}
