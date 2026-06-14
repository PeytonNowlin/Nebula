mod builtins;
mod checker;
mod diagnostic_extract;
mod error;
mod expr;
mod program;
mod report;
mod resolve;
mod scope;
mod util;

use nebula_ast::Program;

pub use error::TypeError;
pub use program::{FnInfo, ProbeInfo, StructInfo, TypedProgram};
pub use report::{diagnostics_from_type_errors, report_with_source, TypecheckErrors};

use checker::Checker;

pub fn typecheck(program: &Program) -> Result<TypedProgram, TypecheckErrors> {
    let mut checker = Checker::new();
    let mut errors = Vec::new();

    for item in &program.items {
        checker.collect_top_level(&item.node, &mut errors);
    }

    if !checker.has_main {
        errors.push(TypeError::MissingMain { span: 0..0 });
    }

    if !errors.is_empty() {
        return Err(TypecheckErrors::new(errors));
    }

    for item in &program.items {
        checker.check_top_level(&item.node, &mut errors);
    }

    if errors.is_empty() {
        Ok(TypedProgram {
            program: program.clone(),
            functions: checker.functions,
            structs: checker.structs,
            probes: checker.probes,
            has_main: checker.has_main,
        })
    } else {
        Err(TypecheckErrors::new(errors))
    }
}