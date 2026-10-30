use lira::{Arch, Instruction, Snippet, StatementSeq};

pub fn iter_arch_stmt_seq_part<'a>(
    instructions: &'a [Instruction],
    snippets: &'a [Snippet],
) -> impl Iterator<Item = (&'a str, &'a StatementSeq)> {
    [].into_iter()
        .chain(instructions.iter().map(|i| (i.name.as_str(), &i.semantic)))
        .chain(snippets.iter().map(|s| (s.name.as_str(), &s.seq)))
}

pub fn iter_arch_stmt_seq(arch: &Arch) -> impl Iterator<Item = (&str, &StatementSeq)> {
    iter_arch_stmt_seq_part(&arch.instructions, &arch.snippets)
}

pub fn iter_arch_stmt_seq_mut_part<'a>(
    instructions: &'a mut [Instruction],
    snippets: &'a mut [Snippet],
) -> impl Iterator<Item = (&'a str, &'a mut StatementSeq)> {
    [].into_iter()
        .chain(instructions.iter_mut().map(|i| (i.name.as_str(), &mut i.semantic)))
        .chain(snippets.iter_mut().map(|s| (s.name.as_str(), &mut s.seq)))
}

pub fn iter_arch_stmt_seq_mut(
    arch: &mut Arch,
) -> impl Iterator<Item = (&str, &mut StatementSeq)> {
    iter_arch_stmt_seq_mut_part(&mut arch.instructions, &mut arch.snippets)
}
