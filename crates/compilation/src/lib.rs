use evm_glue::{assembly::Asm, opcodes::Opcode, utils::MarkTracker};
use huff_analysis::label_stack::LabelStack;
use huff_ast::{
    u256_as_push, Definition, IdentifiableNode, Instruction, Invoke, Macro, MacroStatement,
};
use std::collections::BTreeMap;

pub fn generate_for_entrypoint<'ast>(
    global_defs: &BTreeMap<&str, &Definition<'ast>>,
    entry_point: &'ast Macro,
    mark_tracker: &'ast mut MarkTracker,
    config: &CompileConfig,
) -> Result<Vec<Asm>, String> {
    let mut label_stack: LabelStack<usize> = LabelStack::default();
    let mut asm = Vec::with_capacity(10_000);

    generate_for_macro(
        global_defs,
        entry_point,
        Box::new([]),
        mark_tracker,
        &mut label_stack,
        &mut asm,
        config,
    )?;

    Ok(asm)
}

fn generate_for_macro<'ast, 'src>(
    global_defs: &BTreeMap<&str, &'ast Definition<'src>>,
    current: &'ast Macro<'src>,
    arg_values: Box<[Asm]>,
    mark_tracker: &'ast mut MarkTracker,
    label_stack: &'ast mut LabelStack<'src, usize>,
    asm: &mut Vec<Asm>,
    config: &CompileConfig,
) -> Result<(), String> {
    let current_args: BTreeMap<&str, Asm> = BTreeMap::from_iter(
        current
            .args
            .0
            .iter()
            .map(|name| name.ident())
            .zip(arg_values),
    );

    label_stack.enter_context();

    current.body.iter().for_each(|stmt| {
        if let MacroStatement::LabelDefinition(name) = stmt {
            label_stack.push(name.ident(), mark_tracker.next_mark());
        }
    });

    current.body.iter().try_for_each(|stmt| {
        match stmt {
            MacroStatement::LabelDefinition(name) => {
                asm.extend([
                    Asm::Mark(*label_stack.get(name.ident()).unwrap()),
                    Asm::Op(Opcode::JUMPDEST),
                ]);
            }
            MacroStatement::Invoke(invoke) => match invoke {
                Invoke::Macro { name, args } => {
                    let target =
                        if let Definition::Macro(target) = global_defs.get(name.ident()).unwrap() {
                            target
                        } else {
                            panic!("Target should've been validated to be macro")
                        };
                    generate_for_macro(
                        global_defs,
                        target,
                        args.0
                            .iter()
                            .map(|arg| instruction_to_asm(&current_args, label_stack, config, arg))
                            .collect::<Result<_, String>>()?,
                        mark_tracker,
                        label_stack,
                        asm,
                        config,
                    )?;
                }
                _ => Err(format!(
                    "Compilation not yet implemented for this invocation type {:?}",
                    invoke
                ))?,
            },
            MacroStatement::Instruction(i) => {
                asm.push(instruction_to_asm(&current_args, label_stack, config, i)?)
            }
        };
        Result::<(), String>::Ok(())
    })?;

    label_stack.leave_context();

    Ok(())
}

fn instruction_to_asm(
    args: &BTreeMap<&str, Asm>,
    label_stack: &LabelStack<usize>,
    config: &CompileConfig,
    i: &Instruction,
) -> Result<Asm, String> {
    match i {
        Instruction::Op((op, _)) => Ok(Asm::Op(*op)),
        Instruction::VariablePush((value, _)) => {
            if value.byte_len() == 0 && config.allow_push0 {
                Ok(Asm::Op(Opcode::PUSH0))
            } else {
                Ok(Asm::Op(u256_as_push(*value)))
            }
        }
        Instruction::LabelReference(name) => Ok(Asm::mref(*label_stack.get(name.ident()).unwrap())),
        Instruction::ConstantReference(name) => Err(format!(
            "Invalid reference to constant '{}' (not yet supported)",
            name.0
        )),
        Instruction::MacroArgReference(name) => Ok(args.get(name.ident()).unwrap().clone()),
    }
}

#[derive(Debug, Clone)]
pub struct CompileConfig {
    pub allow_push0: bool,
}
