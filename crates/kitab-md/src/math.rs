// SPDX-License-Identifier: AGPL-3.0-or-later

//! TeX math (PLAN.md §3.4): `$…$`, `$$…$$` and ```` ```math ```` read into
//! a small tree the view lays out natively (fractions, roots, scripts, big
//! operators with limits, delimiters, matrices), and a one-line Unicode form
//! for inline math. Not a TeX engine: the common notation of notes and
//! papers. Structural mistakes (an unclosed brace, a missing argument) are
//! errors, shown as the source with a mark; an unknown command shows as its
//! name. Hostile input is bounded in depth and size.

/// Deeper nesting than this is refused.
const MAX_DEPTH: usize = 48;
/// More tokens than this is refused.
const MAX_TOKENS: usize = 8192;

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Row(Vec<Node>),
    /// A letter or Greek letter: italic.
    Ident(String),
    /// Digits: upright.
    Num(String),
    /// An operator, relation or punctuation: upright, spaced by the view.
    Op(String),
    /// A function name (sin, log): upright, no extra spacing.
    Func(String),
    /// \text{…}: upright prose.
    Text(String),
    Frac(Box<Node>, Box<Node>),
    Sqrt(Option<Box<Node>>, Box<Node>),
    Scripts {
        base: Box<Node>,
        sub: Option<Box<Node>>,
        sup: Option<Box<Node>>,
    },
    /// ∑ ∫ lim… with limits under and over in display math.
    BigOp {
        op: String,
        sub: Option<Box<Node>>,
        sup: Option<Box<Node>>,
    },
    /// \left( … \right) and the like; an empty side is drawn as nothing.
    Fenced {
        open: String,
        close: String,
        body: Box<Node>,
    },
    /// Rows of cells; `open`/`close` are the brackets around it.
    Matrix {
        rows: Vec<Vec<Node>>,
        open: String,
        close: String,
        /// cases: cells sit left.
        left: bool,
    },
    /// A space this many em wide (\, \quad…); negative for \!.
    Space(f32),
    /// \hat{x} and friends: the mark sits over the body.
    Accent {
        body: Box<Node>,
        mark: char,
    },
    Bold(Box<Node>),
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Cmd(String),
    Open,
    Close,
    Sup,
    Sub,
    Amp,
    Char(char),
}

fn tokens(src: &str) -> Result<Vec<Tok>, String> {
    let mut out = Vec::new();
    let mut it = src.chars().peekable();
    while let Some(c) = it.next() {
        if out.len() > MAX_TOKENS {
            return Err("too long".into());
        }
        match c {
            '\\' => {
                let mut name = String::new();
                while let Some(&n) = it.peek() {
                    if n.is_ascii_alphabetic() {
                        name.push(n);
                        it.next();
                    } else {
                        break;
                    }
                }
                if name.is_empty() {
                    match it.next() {
                        Some(n) => name.push(n),
                        None => return Err("a \\ at the end".into()),
                    }
                }
                out.push(Tok::Cmd(name));
            }
            '{' => out.push(Tok::Open),
            '}' => out.push(Tok::Close),
            '^' => out.push(Tok::Sup),
            '_' => out.push(Tok::Sub),
            '&' => out.push(Tok::Amp),
            c if c.is_whitespace() => {}
            '%' => {
                // A TeX comment runs to the end of the line.
                for n in it.by_ref() {
                    if n == '\n' {
                        break;
                    }
                }
            }
            c => out.push(Tok::Char(c)),
        }
    }
    Ok(out)
}

/// What ends a row.
#[derive(Clone, Copy, PartialEq)]
enum Stop {
    /// The end of the input.
    End,
    /// `}`.
    Brace,
    /// `&`, `\\` or `\end` inside an environment.
    Cell,
    /// `\right`.
    Right,
}

struct Parser {
    toks: Vec<Tok>,
    at: usize,
    depth: usize,
}

fn symbol(name: &str) -> Option<Node> {
    let ident = |s: &str| Some(Node::Ident(s.into()));
    let op = |s: &str| Some(Node::Op(s.into()));
    match name {
        // Greek, lower case italic, upper case upright.
        "alpha" => ident("α"),
        "beta" => ident("β"),
        "gamma" => ident("γ"),
        "delta" => ident("δ"),
        "epsilon" => ident("ϵ"),
        "varepsilon" => ident("ε"),
        "zeta" => ident("ζ"),
        "eta" => ident("η"),
        "theta" => ident("θ"),
        "vartheta" => ident("ϑ"),
        "iota" => ident("ι"),
        "kappa" => ident("κ"),
        "lambda" => ident("λ"),
        "mu" => ident("μ"),
        "nu" => ident("ν"),
        "xi" => ident("ξ"),
        "pi" => ident("π"),
        "varpi" => ident("ϖ"),
        "rho" => ident("ρ"),
        "varrho" => ident("ϱ"),
        "sigma" => ident("σ"),
        "varsigma" => ident("ς"),
        "tau" => ident("τ"),
        "upsilon" => ident("υ"),
        "phi" => ident("ϕ"),
        "varphi" => ident("φ"),
        "chi" => ident("χ"),
        "psi" => ident("ψ"),
        "omega" => ident("ω"),
        "Gamma" => op("Γ"),
        "Delta" => op("Δ"),
        "Theta" => op("Θ"),
        "Lambda" => op("Λ"),
        "Xi" => op("Ξ"),
        "Pi" => op("Π"),
        "Sigma" => op("Σ"),
        "Upsilon" => op("Υ"),
        "Phi" => op("Φ"),
        "Psi" => op("Ψ"),
        "Omega" => op("Ω"),
        "ell" => ident("ℓ"),
        "hbar" => ident("ℏ"),
        "imath" => ident("ı"),
        "Re" => op("ℜ"),
        "Im" => op("ℑ"),
        "aleph" => op("ℵ"),
        // Operators and relations.
        "times" => op("×"),
        "div" => op("÷"),
        "cdot" => op("·"),
        "pm" => op("±"),
        "mp" => op("∓"),
        "ast" => op("∗"),
        "star" => op("⋆"),
        "circ" => op("∘"),
        "bullet" => op("∙"),
        "oplus" => op("⊕"),
        "otimes" => op("⊗"),
        "le" | "leq" => op("≤"),
        "ge" | "geq" => op("≥"),
        "ne" | "neq" => op("≠"),
        "ll" => op("≪"),
        "gg" => op("≫"),
        "approx" => op("≈"),
        "sim" => op("∼"),
        "simeq" => op("≃"),
        "cong" => op("≅"),
        "equiv" => op("≡"),
        "propto" => op("∝"),
        "in" => op("∈"),
        "notin" => op("∉"),
        "ni" => op("∋"),
        "subset" => op("⊂"),
        "subseteq" => op("⊆"),
        "supset" => op("⊃"),
        "supseteq" => op("⊇"),
        "cup" => op("∪"),
        "cap" => op("∩"),
        "setminus" => op("∖"),
        "emptyset" | "varnothing" => op("∅"),
        "forall" => op("∀"),
        "exists" => op("∃"),
        "nexists" => op("∄"),
        "neg" | "lnot" => op("¬"),
        "land" | "wedge" => op("∧"),
        "lor" | "vee" => op("∨"),
        "partial" => op("∂"),
        "nabla" => op("∇"),
        "infty" => op("∞"),
        "to" | "rightarrow" => op("→"),
        "leftarrow" | "gets" => op("←"),
        "leftrightarrow" => op("↔"),
        "Rightarrow" | "implies" => op("⇒"),
        "Leftarrow" => op("⇐"),
        "Leftrightarrow" | "iff" => op("⇔"),
        "mapsto" => op("↦"),
        "uparrow" => op("↑"),
        "downarrow" => op("↓"),
        "ldots" | "dots" => op("…"),
        "cdots" => op("⋯"),
        "vdots" => op("⋮"),
        "ddots" => op("⋱"),
        "prime" => op("′"),
        "degree" => op("°"),
        "angle" => op("∠"),
        "perp" => op("⊥"),
        "parallel" => op("∥"),
        "mid" => op("∣"),
        "langle" => op("⟨"),
        "rangle" => op("⟩"),
        "lfloor" => op("⌊"),
        "rfloor" => op("⌋"),
        "lceil" => op("⌈"),
        "rceil" => op("⌉"),
        "{" => op("{"),
        "}" => op("}"),
        "|" => op("‖"),
        "%" => op("%"),
        "$" => op("$"),
        "#" => op("#"),
        "&" => op("&"),
        "_" => op("_"),
        _ => None,
    }
}

fn big_op(name: &str) -> Option<&'static str> {
    Some(match name {
        "sum" => "∑",
        "prod" => "∏",
        "coprod" => "∐",
        "int" => "∫",
        "iint" => "∬",
        "iiint" => "∭",
        "oint" => "∮",
        "bigcup" => "⋃",
        "bigcap" => "⋂",
        "bigoplus" => "⨁",
        "bigotimes" => "⨂",
        "lim" => "lim",
        "limsup" => "lim sup",
        "liminf" => "lim inf",
        "max" => "max",
        "min" => "min",
        "sup" => "sup",
        "inf" => "inf",
        "argmax" => "arg max",
        "argmin" => "arg min",
        _ => return None,
    })
}

fn func(name: &str) -> bool {
    matches!(
        name,
        "sin"
            | "cos"
            | "tan"
            | "cot"
            | "sec"
            | "csc"
            | "arcsin"
            | "arccos"
            | "arctan"
            | "sinh"
            | "cosh"
            | "tanh"
            | "log"
            | "ln"
            | "lg"
            | "exp"
            | "det"
            | "dim"
            | "ker"
            | "gcd"
            | "deg"
            | "arg"
            | "hom"
            | "Pr"
            | "mod"
            | "bmod"
    )
}

fn accent(name: &str) -> Option<char> {
    Some(match name {
        "hat" | "widehat" => '\u{302}',
        "bar" | "overline" => '\u{305}',
        "vec" => '\u{20D7}',
        "dot" => '\u{307}',
        "ddot" => '\u{308}',
        "tilde" | "widetilde" => '\u{303}',
        "check" => '\u{30C}',
        "breve" => '\u{306}',
        "acute" => '\u{301}',
        "grave" => '\u{300}',
        _ => return None,
    })
}

fn double_struck(c: char) -> char {
    match c {
        'R' => 'ℝ',
        'N' => 'ℕ',
        'Z' => 'ℤ',
        'Q' => 'ℚ',
        'C' => 'ℂ',
        'P' => 'ℙ',
        'H' => 'ℍ',
        'A'..='Z' => char::from_u32('𝔸' as u32 + (c as u32 - 'A' as u32)).unwrap_or(c),
        c => c,
    }
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.at).cloned();
        self.at += 1;
        t
    }

    fn enter(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            Err("nested too deeply".into())
        } else {
            Ok(())
        }
    }

    /// Atoms up to `stop`, with scripts attached.
    fn row(&mut self, stop: Stop) -> Result<Vec<Node>, String> {
        self.enter()?;
        let mut out: Vec<Node> = Vec::new();
        loop {
            match self.peek() {
                None => {
                    if stop == Stop::End {
                        break;
                    }
                    return Err(match stop {
                        Stop::Brace => "a { is never closed".into(),
                        Stop::Right => "\\left without \\right".into(),
                        _ => "an environment is never ended".into(),
                    });
                }
                Some(Tok::Close) => {
                    if stop == Stop::Brace {
                        self.next();
                        break;
                    }
                    return Err("a } without its {".into());
                }
                Some(Tok::Amp) if stop == Stop::Cell => break,
                Some(Tok::Cmd(c)) if stop == Stop::Cell && (c == "\\" || c == "end") => break,
                Some(Tok::Cmd(c)) if stop == Stop::Right && c == "right" => break,
                Some(Tok::Amp) => {
                    self.next();
                    out.push(Node::Space(0.5));
                }
                Some(Tok::Cmd(c)) if c == "\\" => {
                    // A line break outside an environment: a wide space.
                    self.next();
                    out.push(Node::Space(1.0));
                }
                Some(Tok::Sup) | Some(Tok::Sub) => {
                    let sup = self.next() == Some(Tok::Sup);
                    let arg = self.arg()?;
                    let base = out.pop().unwrap_or(Node::Row(Vec::new()));
                    out.push(attach(base, arg, sup)?);
                }
                Some(_) => {
                    let atom = self.atom()?;
                    out.push(atom);
                }
            }
        }
        self.depth -= 1;
        Ok(out)
    }

    /// One argument: a group or a single token.
    fn arg(&mut self) -> Result<Node, String> {
        match self.peek() {
            None | Some(Tok::Close) | Some(Tok::Amp) => Err("an argument is missing".into()),
            Some(Tok::Sup) | Some(Tok::Sub) => Err("a script has nothing to attach to".into()),
            _ => self.atom(),
        }
    }

    /// The raw text of a `{…}` group (for \text and environment names).
    fn raw_group(&mut self) -> Result<String, String> {
        if self.next() != Some(Tok::Open) {
            return Err("a { is missing".into());
        }
        let mut s = String::new();
        let mut level = 0;
        loop {
            match self.next() {
                None => return Err("a { is never closed".into()),
                Some(Tok::Close) if level == 0 => break,
                Some(Tok::Close) => {
                    level -= 1;
                    s.push('}');
                }
                Some(Tok::Open) => {
                    level += 1;
                    s.push('{');
                }
                Some(Tok::Char(c)) => s.push(c),
                Some(Tok::Cmd(c)) if c == " " || c == "," || c == ";" => s.push(' '),
                Some(Tok::Cmd(c)) => s.push_str(&c),
                Some(Tok::Sup) => s.push('^'),
                Some(Tok::Sub) => s.push('_'),
                Some(Tok::Amp) => s.push('&'),
            }
        }
        Ok(s)
    }

    /// A delimiter after \left or \right: "(" or "\{" or "." (none).
    fn delim(&mut self) -> Result<String, String> {
        match self.next() {
            Some(Tok::Char('.')) => Ok(String::new()),
            Some(Tok::Char(c)) => Ok(c.to_string()),
            Some(Tok::Cmd(c)) => match symbol(&c) {
                Some(Node::Op(s)) => Ok(s),
                _ if c == "|" => Ok("‖".into()),
                _ => Err(format!("\\{c} isn't a delimiter")),
            },
            _ => Err("a delimiter is missing".into()),
        }
    }

    fn atom(&mut self) -> Result<Node, String> {
        match self.next() {
            None => Err("an argument is missing".into()),
            Some(Tok::Open) => Ok(Node::Row(self.row(Stop::Brace)?)),
            Some(Tok::Char(c)) if c.is_ascii_digit() || c == '.' => {
                let mut s = c.to_string();
                while let Some(Tok::Char(n)) = self.peek() {
                    if n.is_ascii_digit() || *n == '.' {
                        s.push(*n);
                        self.next();
                    } else {
                        break;
                    }
                }
                Ok(Node::Num(s))
            }
            Some(Tok::Char(c)) if c.is_alphabetic() => Ok(Node::Ident(c.to_string())),
            Some(Tok::Char(c)) => Ok(Node::Op(match c {
                '-' => "−".into(),
                '*' => "∗".into(),
                '\'' => "′".into(),
                c => c.to_string(),
            })),
            Some(Tok::Cmd(name)) => self.command(&name),
            Some(t) => Err(format!("unexpected {t:?}")),
        }
    }

    fn command(&mut self, name: &str) -> Result<Node, String> {
        if let Some(n) = symbol(name) {
            return Ok(n);
        }
        if let Some(op) = big_op(name) {
            return Ok(Node::BigOp {
                op: op.into(),
                sub: None,
                sup: None,
            });
        }
        if func(name) {
            return Ok(Node::Func(name.into()));
        }
        if let Some(mark) = accent(name) {
            return Ok(Node::Accent {
                body: Box::new(self.arg()?),
                mark,
            });
        }
        Ok(match name {
            "frac" | "dfrac" | "tfrac" | "cfrac" => {
                let num = self.arg()?;
                let den = self.arg()?;
                Node::Frac(Box::new(num), Box::new(den))
            }
            "binom" => {
                let n = self.arg()?;
                let k = self.arg()?;
                Node::Fenced {
                    open: "(".into(),
                    close: ")".into(),
                    body: Box::new(Node::Matrix {
                        rows: vec![vec![n], vec![k]],
                        open: String::new(),
                        close: String::new(),
                        left: false,
                    }),
                }
            }
            "sqrt" => {
                let index = if self.peek() == Some(&Tok::Char('[')) {
                    self.next();
                    let mut idx = Vec::new();
                    loop {
                        match self.peek() {
                            None => return Err("a [ is never closed".into()),
                            Some(Tok::Char(']')) => {
                                self.next();
                                break;
                            }
                            _ => idx.push(self.atom()?),
                        }
                    }
                    Some(Box::new(Node::Row(idx)))
                } else {
                    None
                };
                Node::Sqrt(index, Box::new(self.arg()?))
            }
            "text" | "textrm" | "mbox" | "mathrm" | "operatorname" | "textit" | "textbf" => {
                Node::Text(self.raw_group()?)
            }
            "mathbf" | "boldsymbol" | "bm" => Node::Bold(Box::new(self.arg()?)),
            "mathit" | "mathcal" | "mathscr" | "mathsf" | "mathtt" | "displaystyle"
            | "textstyle" => self.arg()?,
            "mathbb" => {
                let s = self.raw_group()?;
                Node::Op(s.chars().map(double_struck).collect())
            }
            "left" => {
                let open = self.delim()?;
                let body = self.row(Stop::Right)?;
                self.next(); // \right
                let close = self.delim()?;
                Node::Fenced {
                    open,
                    close,
                    body: Box::new(Node::Row(body)),
                }
            }
            "right" => return Err("\\right without \\left".into()),
            "begin" => self.environment()?,
            "end" => return Err("\\end without \\begin".into()),
            "," | ":" | ">" | "thinspace" => Node::Space(0.17),
            ";" | "medspace" => Node::Space(0.22),
            " " => Node::Space(0.25),
            "!" => Node::Space(-0.17),
            "quad" => Node::Space(1.0),
            "qquad" => Node::Space(2.0),
            "big" | "Big" | "bigg" | "Bigg" | "bigl" | "bigr" | "Bigl" | "Bigr" => {
                Node::Op(self.delim()?)
            }
            // Unknown: show its name rather than lose it.
            other => Node::Func(format!("\\{other}")),
        })
    }

    fn environment(&mut self) -> Result<Node, String> {
        let name = self.raw_group()?;
        let (open, close, left) = match name.as_str() {
            "matrix" | "smallmatrix" | "array" => ("", "", false),
            "pmatrix" => ("(", ")", false),
            "bmatrix" => ("[", "]", false),
            "Bmatrix" => ("{", "}", false),
            "vmatrix" => ("|", "|", false),
            "Vmatrix" => ("‖", "‖", false),
            "cases" => ("{", "", true),
            "aligned" | "align" | "align*" | "gathered" | "split" | "equation" | "equation*" => {
                ("", "", false)
            }
            other => return Err(format!("unknown environment {other}")),
        };
        // array's column spec: {cc|l} — skipped.
        if name == "array" && self.peek() == Some(&Tok::Open) {
            self.raw_group()?;
        }
        let mut rows = vec![Vec::new()];
        loop {
            let cell = self.row(Stop::Cell)?;
            if let Some(r) = rows.last_mut() {
                r.push(Node::Row(cell));
            }
            match self.next() {
                Some(Tok::Amp) => {}
                Some(Tok::Cmd(c)) if c == "\\" => rows.push(Vec::new()),
                Some(Tok::Cmd(c)) if c == "end" => {
                    let end = self.raw_group()?;
                    if end != name {
                        return Err(format!("\\begin{{{name}}} ended by \\end{{{end}}}"));
                    }
                    break;
                }
                _ => return Err(format!("\\begin{{{name}}} is never ended")),
            }
        }
        // A trailing \\ leaves an empty last row.
        if rows.len() > 1
            && rows
                .last()
                .is_some_and(|r| r.iter().all(|c| matches!(c, Node::Row(v) if v.is_empty())))
        {
            rows.pop();
        }
        Ok(Node::Matrix {
            rows,
            open: open.into(),
            close: close.into(),
            left,
        })
    }
}

fn attach(base: Node, arg: Node, sup: bool) -> Result<Node, String> {
    let set = |slot: &mut Option<Box<Node>>, arg: Node| -> Result<(), String> {
        if slot.is_some() {
            return Err(if sup {
                "two superscripts in a row".into()
            } else {
                "two subscripts in a row".into()
            });
        }
        *slot = Some(Box::new(arg));
        Ok(())
    };
    Ok(match base {
        Node::BigOp {
            op,
            mut sub,
            sup: mut sp,
        } => {
            set(if sup { &mut sp } else { &mut sub }, arg)?;
            Node::BigOp { op, sub, sup: sp }
        }
        Node::Scripts {
            base,
            mut sub,
            sup: mut sp,
        } => {
            set(if sup { &mut sp } else { &mut sub }, arg)?;
            Node::Scripts { base, sub, sup: sp }
        }
        base => {
            let (mut sub, mut sp) = (None, None);
            set(if sup { &mut sp } else { &mut sub }, arg)?;
            Node::Scripts {
                base: Box::new(base),
                sub,
                sup: sp,
            }
        }
    })
}

/// Read TeX math into a tree.
pub fn parse(src: &str) -> Result<Node, String> {
    let toks = tokens(src)?;
    let mut p = Parser {
        toks,
        at: 0,
        depth: 0,
    };
    let row = p.row(Stop::End)?;
    Ok(Node::Row(row))
}

fn sup_char(c: char) -> Option<char> {
    Some(match c {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        '+' => '⁺',
        '−' | '-' => '⁻',
        '=' => '⁼',
        '(' => '⁽',
        ')' => '⁾',
        'n' => 'ⁿ',
        'i' => 'ⁱ',
        'T' => 'ᵀ',
        _ => return None,
    })
}

fn sub_char(c: char) -> Option<char> {
    Some(match c {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        '+' => '₊',
        '−' | '-' => '₋',
        '=' => '₌',
        '(' => '₍',
        ')' => '₎',
        'i' => 'ᵢ',
        'j' => 'ⱼ',
        'k' => 'ₖ',
        'n' => 'ₙ',
        'x' => 'ₓ',
        _ => return None,
    })
}

/// Whether a node is one simple symbol (no brackets needed around it).
fn simple(n: &Node) -> bool {
    match n {
        Node::Ident(_) | Node::Num(_) | Node::Op(_) => true,
        Node::Row(v) => v.len() == 1 && simple(&v[0]),
        _ => false,
    }
}

/// The node as one line of Unicode, for inline math: x² rather than x^2,
/// ½-style fractions written a/b.
pub fn to_line(n: &Node) -> String {
    let mut s = String::new();
    line(n, &mut s);
    s
}

fn line(n: &Node, s: &mut String) {
    let script = |arg: &Node, s: &mut String, map: fn(char) -> Option<char>, mark: char| {
        let text = to_line(arg);
        match text.chars().map(map).collect::<Option<String>>() {
            Some(t) if !t.is_empty() => s.push_str(&t),
            _ if simple(arg) => {
                s.push(mark);
                s.push_str(&text);
            }
            _ => {
                s.push(mark);
                s.push('(');
                s.push_str(&text);
                s.push(')');
            }
        }
    };
    match n {
        Node::Row(v) => v.iter().for_each(|c| line(c, s)),
        Node::Ident(t) | Node::Num(t) | Node::Text(t) => s.push_str(t),
        Node::Func(t) => {
            s.push_str(t);
            s.push(' ');
        }
        Node::Op(t) => {
            let spaced = matches!(
                t.as_str(),
                "=" | "+"
                    | "−"
                    | "<"
                    | ">"
                    | "≤"
                    | "≥"
                    | "≠"
                    | "≈"
                    | "→"
                    | "⇒"
                    | "×"
                    | "±"
                    | "∈"
            );
            if spaced {
                s.push(' ');
                s.push_str(t);
                s.push(' ');
            } else {
                s.push_str(t);
            }
        }
        Node::Frac(a, b) => {
            let (a, b) = (to_line(a), to_line(b));
            let wrap = |t: String| {
                if t.chars().count() > 1 && t.contains(|c: char| " +−".contains(c)) {
                    format!("({t})")
                } else {
                    t
                }
            };
            s.push_str(&wrap(a));
            s.push('/');
            s.push_str(&wrap(b));
        }
        Node::Sqrt(idx, body) => {
            if let Some(i) = idx {
                s.push_str(&to_line(i));
            }
            s.push('√');
            if simple(body) {
                s.push_str(&to_line(body));
            } else {
                s.push('(');
                s.push_str(&to_line(body));
                s.push(')');
            }
        }
        Node::Scripts { base, sub, sup } => {
            line(base, s);
            if let Some(b) = sub {
                script(b, s, sub_char, '_');
            }
            if let Some(p) = sup {
                script(p, s, sup_char, '^');
            }
        }
        Node::BigOp { op, sub, sup } => {
            s.push_str(op);
            if let Some(b) = sub {
                script(b, s, sub_char, '_');
            }
            if let Some(p) = sup {
                script(p, s, sup_char, '^');
            }
            s.push(' ');
        }
        Node::Fenced { open, close, body } => {
            s.push_str(open);
            line(body, s);
            s.push_str(close);
        }
        Node::Matrix {
            rows, open, close, ..
        } => {
            s.push_str(if open.is_empty() { "[" } else { open });
            let rows: Vec<String> = rows
                .iter()
                .map(|r| r.iter().map(to_line).collect::<Vec<_>>().join(", "))
                .collect();
            s.push_str(&rows.join("; "));
            s.push_str(if close.is_empty() { "]" } else { close });
        }
        Node::Space(em) => {
            if *em > 0.0 {
                s.push(' ');
            }
        }
        Node::Accent { body, mark } => {
            s.push_str(&to_line(body));
            s.push(*mark);
        }
        Node::Bold(b) => line(b, s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_math_reads_as_unicode() {
        let l = |src: &str| to_line(&parse(src).expect(src));
        assert_eq!(l(r"A = \pi r^2"), "A = πr²");
        assert_eq!(l(r"e^{i\pi} + 1 = 0"), "e^(iπ) + 1 = 0");
        assert_eq!(l(r"x_1 + x_2"), "x₁ + x₂");
        assert_eq!(l(r"\frac{1}{3}"), "1/3");
        assert_eq!(l(r"\sqrt{2}"), "√2");
        assert_eq!(l(r"\alpha \le \beta"), "α ≤ β");
    }

    #[test]
    fn display_math_builds_a_tree() {
        let n = parse(r"\int_0^1 x^2 \, dx = \frac{1}{3}").expect("parses");
        let Node::Row(v) = n else { panic!("a row") };
        assert!(matches!(&v[0], Node::BigOp { op, sub: Some(_), sup: Some(_) } if op == "∫"));
        assert!(v.iter().any(|n| matches!(n, Node::Frac(..))));

        let m = parse(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}").expect("parses");
        let Node::Row(v) = m else { panic!("a row") };
        let Node::Matrix {
            rows, open, close, ..
        } = &v[0]
        else {
            panic!("a matrix")
        };
        assert_eq!((rows.len(), rows[0].len()), (2, 2));
        assert_eq!((open.as_str(), close.as_str()), ("(", ")"));

        let s = parse(r"\sum_{k=1}^{n} k = \frac{n(n+1)}{2}").expect("parses");
        assert!(matches!(&s, Node::Row(v) if matches!(&v[0], Node::BigOp { op, .. } if op == "∑")));
    }

    #[test]
    fn mistakes_are_errors_not_panics() {
        assert!(parse(r"\frac{1}{").is_err());
        assert!(parse(r"x}").is_err());
        assert!(parse(r"x^").is_err());
        assert!(parse(r"x^1^2").is_err());
        assert!(parse(r"\left( x").is_err());
        assert!(parse(r"\begin{pmatrix} a \end{bmatrix}").is_err());
        assert!(parse(r"\begin{weird} a \end{weird}").is_err());
        assert!(parse(&"{".repeat(500)).is_err());
        assert!(parse(&"x".repeat(MAX_TOKENS + 10)).is_err());
    }

    #[test]
    fn unknown_commands_show_their_name() {
        let n = parse(r"\foo x").expect("parses");
        assert_eq!(to_line(&n), r"\foo x");
    }

    #[test]
    fn cases_and_left_right() {
        let n =
            parse(r"f(x) = \begin{cases} 0 & x < 0 \\ 1 & x \ge 0 \end{cases}").expect("parses");
        assert!(to_line(&n).contains("x ≥ 0"));
        let f = parse(r"\left[ \frac{a}{b} \right)").expect("parses");
        assert_eq!(to_line(&f), "[a/b)");
    }
}
