use lira::{Arch, Instruction, Snippet, StatementSeq};

pub fn iter_stmt_seq_part<'a>(
    instructions: &'a [Instruction],
    snippets: &'a [Snippet],
) -> impl Iterator<Item = &'a StatementSeq> {
    [].into_iter()
        .chain(instructions.iter().map(|i| &i.semantic))
        .chain(snippets.iter().map(|s| &s.seq))
}

pub fn iter_stmt_seq(arch: &Arch) -> impl Iterator<Item = &StatementSeq> {
    iter_stmt_seq_part(&arch.instructions, &arch.snippets)
}

pub fn iter_stmt_seq_mut_part<'a>(
    instructions: &'a mut [Instruction],
    snippets: &'a mut [Snippet],
) -> impl Iterator<Item = (&'a String, &'a mut StatementSeq)> {
    [].into_iter()
        .chain(instructions.iter_mut().map(|i| (&i.name, &mut i.semantic)))
        .chain(snippets.iter_mut().map(|s| (&s.name, &mut s.seq)))
}

pub fn iter_stmt_seq_mut(arch: &mut Arch) -> impl Iterator<Item = (&String, &mut StatementSeq)> {
    iter_stmt_seq_mut_part(&mut arch.instructions, &mut arch.snippets)
}
