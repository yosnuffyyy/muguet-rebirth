//! Host-only regression through the real import dispatcher and guest memory.
use crate::{HostState, Runtime};
use anyhow::{ensure, Context, Result};
use std::sync::Arc;
use wasmparser::{Operator, Parser, Payload, TypeRef};

pub fn verify(rt: &Arc<Runtime>, original: &[u8]) -> Result<()> {
    use crate::rebirth::input_abi::{CONTROLS, INPUT_BYTES, PAD};
    let (mut imports, mut input, mut function) = (0, None, 0);
    let (mut pad_hook, mut control_hook) = (false, false);
    for payload in Parser::new(0).parse_all(original) {
        match payload? {
            Payload::ImportSection(section) => {
                for imp in section.into_imports() {
                    let imp = imp?;
                    if matches!(imp.ty, TypeRef::Func(_)) {
                        if imp.name == "wasm_input_publish_js" { input = Some(imports); }
                        imports += 1;
                    }
                }
                function = imports;
            }
            Payload::CodeSectionEntry(body) => {
                // Names/indices established in the pinned module; CI verifies its SHA256.
                if function == 65359 || function == 65444 {
                    let mut previous = Vec::new();
                    let mut reader = body.get_operators_reader()?;
                    while !reader.eof() {
                        let op = reader.read()?;
                        if matches!(op, Operator::Call { function_index } if Some(function_index) == input) {
                            if function == 65444 && previous.last() == Some(&format!("const:{}", -1 - PAD as i64)) { pad_hook = true; }
                            if function == 65359 && previous == ["const:-1", "local:0", "sub"] { control_hook = true; }
                        }
                        previous.push(match op {
                            Operator::I64Const { value } => format!("const:{value}"),
                            Operator::LocalGet { local_index } => format!("local:{local_index}"),
                            Operator::I64Sub => "sub".to_string(),
                            _ => "other".to_string(),
                        });
                        if previous.len() > 3 { previous.remove(0); }
                    }
                }
                function += 1;
            }
            _ => {}
        }
    }
    ensure!(pad_hook && control_hook, "pinned engine tagged input call sequences changed");
    let index = rt.imports.iter().position(|i| i.name == "wasm_input_publish_js").context("input import missing")?;
    // Reproduce the exact signed i64 host call with a tiny executable Wasm caller.
    use wasm_encoder::*;
    let mut module = Module::new();
    let mut types = TypeSection::new(); types.ty().function([ValType::I64], []); module.section(&types);
    let mut imports = ImportSection::new(); imports.import("env", "wasm_input_publish_js", EntityType::Function(0)); module.section(&imports);
    let mut functions = FunctionSection::new(); functions.function(0); module.section(&functions);
    let mut exports = ExportSection::new(); exports.export("call", ExportKind::Func, 1); module.section(&exports);
    let mut function = Function::new([]); function.instruction(&Instruction::LocalGet(0)).instruction(&Instruction::Call(0)).instruction(&Instruction::End);
    let mut code = CodeSection::new(); code.function(&function); module.section(&code);
    let module = wasmtime::Module::new(&rt.engine, module.finish())?;
    let mut linker = wasmtime::Linker::new(&rt.engine);
    linker.func_new("env", "wasm_input_publish_js", rt.imports[index].ty.clone(), move |caller, args, results| {
        crate::host::call(caller, index, args, results).map_err(wasmtime::Error::from_anyhow)
    })?;
    let mut store = wasmtime::Store::new(&rt.engine, HostState { pthread: 0 });
    let instance = linker.instantiate(&mut store, &module)?;
    let call = instance.get_typed_func::<i64, ()>(&mut store, "call")?;
    for (width, height) in [(640, 360), (1280, 720)] {
        crate::input::set_size(width, height);
        for mode in [1, 2, 1, 2] {
            std::env::set_var("MUGUET_START_MODE", mode.to_string());
            call.call(&mut store, 0x64748c)?;
            crate::input::key("KeyW", true, false, None, false);
            let before = crate::read_bytes(&rt.mem, 0x64748c, INPUT_BYTES);
            for pointer in [PAD, CONTROLS[0], CONTROLS[1], CONTROLS[2]] {
                call.call(&mut store, -1 - pointer as i64)?;
                ensure!(crate::read_bytes(&rt.mem, 0x64748c, INPUT_BYTES) == before, "tagged callback mutated input state");
                ensure!(crate::input::start_mode() == mode, "tagged callback corrupted mode/pointer");
            }
            for invalid in [0, 3, -1, i64::MIN, i64::MAX] { ensure!(call.call(&mut store, invalid).is_err(), "invalid input descriptor accepted"); }
            crate::input::key("KeyW", false, false, None, false);
            ensure!(crate::read_bytes(&rt.mem, 0x64748c + 0x57, 1) == [0], "key release lost");
        }
    }
    eprintln!("[rebirth-check] PASS: original ControlMgr/CControl negative input hooks verified; actual Wasmtime host dispatch, 2 modes x 2 resolutions x repeated publications, held/released key, invalid pointers; not gameplay proof");
    Ok(())
}
