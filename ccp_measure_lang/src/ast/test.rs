use super::{Expr, Prim, Op};

#[test]
fn atom() {
    let foo = b"1";
    let er = Expr::new(foo);
    let e = er.unwrap();
    assert_eq!(e, vec![Expr::Atom(Prim::Num(1))]);

    let foo = b"1 ";
    let er = Expr::new(foo);
    let e = er.unwrap();
    assert_eq!(e, vec![Expr::Atom(Prim::Num(1))]);

    let foo = b"+";
    let er = Expr::new(foo);
    match er {
        Ok(e) => panic!("false ok: {:?}", e),
        Err(_) => (),
    }

    let foo = b"true";
    let er = Expr::new(foo);
    let e = er.unwrap();
    assert_eq!(e, vec![Expr::Atom(Prim::Bool(true))]);

    let foo = b"false";
    let er = Expr::new(foo);
    let e = er.unwrap();
    assert_eq!(e, vec![Expr::Atom(Prim::Bool(false))]);

    let foo = b"x";
    let er = Expr::new(foo);
    let e = er.unwrap();
    assert_eq!(e, vec![Expr::Atom(Prim::Name(String::from("x")))]);

    let foo = b"acbdefg";
    let er = Expr::new(foo);
    let e = er.unwrap();
    assert_eq!(e, vec![Expr::Atom(Prim::Name(String::from("acbdefg")))]);
}

#[test]
fn simple() {
    let foo = b"(+ 10 20)";
    let er = Expr::new(foo);
    let e = er.unwrap();
    assert_eq!(
        e,
        vec![
            Expr::Sexp(
                Op::Add,
                Box::new(Expr::Atom(Prim::Num(10))),
                Box::new(Expr::Atom(Prim::Num(20)))
            ),
        ]
    );

    let foo = b"(blah 10 20)";
    let er = Expr::new(foo);
    match er {
        Ok(e) => panic!("false ok: {:?}", e),
        Err(_) => (),
    }
}

#[test]
fn tree() {
    let foo = b"(+ (+ 7 3) (+ 4 6))";
    let er = Expr::new(foo);
    let e = er.unwrap();
    assert_eq!(
        e,
        vec![
            Expr::Sexp(
                Op::Add,
                Box::new(Expr::Sexp(
                    Op::Add,
                    Box::new(Expr::Atom(Prim::Num(7))),
                    Box::new(Expr::Atom(Prim::Num(3))),
                )),
                Box::new(Expr::Sexp(
                    Op::Add,
                    Box::new(Expr::Atom(Prim::Num(4))),
                    Box::new(Expr::Atom(Prim::Num(6))),
                ))
            ),
        ]
    );

    let foo = b"(+ (- 17 7) (+ 4 (- 26 20)))";
    let er = Expr::new(foo);
    let e = er.unwrap();
    assert_eq!(
        e,
        vec![
            Expr::Sexp(
                Op::Add,
                Box::new(Expr::Sexp(
                    Op::Sub,
                    Box::new(Expr::Atom(Prim::Num(17))),
                    Box::new(Expr::Atom(Prim::Num(7))),
                )),
                Box::new(Expr::Sexp(
                    Op::Add,
                    Box::new(Expr::Atom(Prim::Num(4))),
                    Box::new(Expr::Sexp(
                        Op::Sub,
                        Box::new(Expr::Atom(Prim::Num(26))),
                        Box::new(Expr::Atom(Prim::Num(20))),
                    )),
                ))
            ),
        ]
    );
}

#[test]
fn whitespace() {
    let foo = b"
            (
                +
                (
                    -
                    17
                    7
                )
                (
                    +
                    4
                    (
                        -
                        26
                        20
                    )
                )
            )";
    let er = Expr::new(foo);
    let e = er.unwrap();
    assert_eq!(
        e,
        vec![
            Expr::Sexp(
                Op::Add,
                Box::new(Expr::Sexp(
                    Op::Sub,
                    Box::new(Expr::Atom(Prim::Num(17))),
                    Box::new(Expr::Atom(Prim::Num(7))),
                )),
                Box::new(Expr::Sexp(
                    Op::Add,
                    Box::new(Expr::Atom(Prim::Num(4))),
                    Box::new(Expr::Sexp(
                        Op::Sub,
                        Box::new(Expr::Atom(Prim::Num(26))),
                        Box::new(Expr::Atom(Prim::Num(20))),
                    )),
                ))
            ),
        ]
    );
}

#[test]
fn operations() {
    let foo = b"(let (:= x (add 40 2)) (if (== x 3) (tup (+ 20 22) (/ 84 2))))";
    let er = Expr::new(foo);
    let e = er.unwrap();
    assert_eq!(
        e,
        vec![
            Expr::Sexp(
                Op::Let,
                Box::new(Expr::Sexp(
                    Op::Bind,
                    Box::new(Expr::Atom(Prim::Name(String::from("x")))),
                    Box::new(Expr::Sexp(
                        Op::Add,
                        Box::new(Expr::Atom(Prim::Num(40))),
                        Box::new(Expr::Atom(Prim::Num(2))),
                    )),
                )),
                Box::new(Expr::Sexp(
                    Op::Beq,
                    Box::new(Expr::Sexp(
                        Op::Equiv,
                        Box::new(Expr::Atom(Prim::Name(String::from("x")))),
                        Box::new(Expr::Atom(Prim::Num(3))),
                    )),
                    Box::new(Expr::Sexp(
                        Op::Tup,
                        Box::new(Expr::Sexp(
                            Op::Add,
                            Box::new(Expr::Atom(Prim::Num(20))),
                            Box::new(Expr::Atom(Prim::Num(22))),
                        )),
                        Box::new(Expr::Sexp(
                            Op::Div,
                            Box::new(Expr::Atom(Prim::Num(84))),
                            Box::new(Expr::Atom(Prim::Num(2))),
                        )),
                    )),
                ))
            ),
        ]
    );
}

use super::Type;
#[test]
fn typecheck() {
    let foo = b"(+ (+ 7 3) (+ 4 6))";
    let er = Expr::new(foo);
    let e = er.unwrap();
    assert_eq!(e.len(), 1);
    let t = e[0].check_type().unwrap();
    assert_eq!(t, Type::Num);
}

#[test]
fn typecheck1() {
    let foo = b"(+ (if (== 3 3) (tup 4 5)) 6)";
    let er = Expr::new(foo);
    let e = er.unwrap();
    assert_eq!(e.len(), 1);
    let t = e[0].check_type().unwrap();
    assert_eq!(t, Type::Num);
}

#[test]
fn bindcheck() {
    let foo = b"(let (:= x 40) (+ x (let (bind y (- 3 2)) (- 3 y))))";
    let er = Expr::new(foo);
    let e = er.unwrap();
    assert_eq!(e.len(), 1);
    let t = e[0].check_type().unwrap();
    assert_eq!(t, Type::Num);

    let foo = b"(let (:= x 40) (+ x y))";
    let er = Expr::new(foo);
    let e = er.unwrap();
    assert_eq!(e.len(), 1);
    let t = e[0].check_type();
    match t {
        Ok(e) => panic!("false ok: {:?}", e),
        Err(_) => (),
    }
}

#[test]
fn defs() {
    let foo = b"(def (Foo 0) (Bar 0) (Baz 0))";
    use nom::{IResult, Needed};
    match super::defs(foo) {
        IResult::Done(r, me) => {
            assert_eq!(r, &[]);
            assert_eq!(
                me,
                vec![
                    (Type::Name(String::from("Foo")), Type::Num),
                    (Type::Name(String::from("Bar")), Type::Num),
                    (Type::Name(String::from("Baz")), Type::Num),
                ]
            );
        }
        IResult::Error(e) => panic!(e),
        IResult::Incomplete(Needed::Unknown) => panic!("incomplete"),
        IResult::Incomplete(Needed::Size(s)) => panic!("need {} more bytes", s),
    }
}

use super::Prog;
#[test]
fn prog() {
    let foo = b"
    (def (foo 0) (baz 0))
    (bind Flow.foo 4)
    (bind bar 5)
    (+ Flow.foo bar)
    (bind Flow.baz (+ Flow.foo (+ bar 6)))
    ";
    let (p, sc) = Prog::new_with_scope(foo).unwrap();
    assert_eq!(p.0.len(), 3);
    assert_eq!(
        sc.get(&Type::Name(String::from("Flow.baz"))),
        Some(&Type::Num)
    );
    assert_eq!(
        sc.get(&Type::Name(String::from("Flow.foo"))),
        Some(&Type::Num)
    );
}

#[test]
fn prog1() {
    let foo = b"
    (def (bar 0) (baz 0))
    (bind x 15)
    (bind Flow.bar (> 3 5))
    (bind Flow.baz (if Flow.bar (tup 6 7)))
    ";
    let (p, sc) = Prog::new_with_scope(foo).unwrap();
    assert_eq!(p.0.len(), 3);
    assert_eq!(
        sc.get(&Type::Name(String::from("Flow.bar"))),
        Some(&Type::Bool)
    );
    assert_eq!(
        sc.get(&Type::Name(String::from("Flow.baz"))),
        Some(&Type::Num)
    );
    assert_eq!(sc.get(&Type::Name(String::from("Flow.foo"))), None);
    assert_eq!(sc.get(&Type::Name(String::from("bar"))), None);
}

#[test]
fn prog2() {
    let foo = b"
    (def (Foo 0) (Ack 0) (MinRtt 10000000))
    (bind Flow.Ack (max Flow.Ack Ack))
    (bind Flow.MinRtt (min Flow.MinRtt Rtt))
    (bind bdp (* Flow.MinRtt SndRate))
    (bind Flow.Foo (* bdp 2))
    ";
    let (p, sc) = Prog::new_with_scope(foo).unwrap();
    assert_eq!(p.0.len(), 4);
    assert_eq!(
        sc.get(&Type::Name(String::from("Flow.Ack"))),
        Some(&Type::Num)
    );
}
