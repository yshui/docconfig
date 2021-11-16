use docconfig::DocConfig;

#[derive(DocConfig)]
struct A {
    /// test
    a: f32
}

/// overall doc
#[derive(DocConfig)]
struct B {
    /// inner
    a: A
}

#[test]
fn test() {
    A::write_doc_config(&mut std::io::stdout(), &[], None).unwrap();
    B::write_doc_config(&mut std::io::stdout(), &[], None).unwrap()
}
