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
fn many_simple() {
    let foo = b"
    (bind foo 4)
    (bind bar 5)
    (bind baz 6)
    ";
    let er = Expr::new(foo);
    let e = er.unwrap();
    assert_eq!(e.len(), 3);
}

#[test]
fn many_vars() {
    let foo = b"
    (bind x 15)
    (bind foo.bar (+ 3 x))
    (bind foo.baz (+ foo.bar x))
    ";
    let er = Expr::new(foo);
    let e = er.unwrap();
    assert_eq!(e.len(), 3);
}
