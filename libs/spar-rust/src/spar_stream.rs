//! This module implements the parsing of SparStream

use std::num::NonZeroU32;

use proc_macro2::{Delimiter, Group, Span, TokenStream, TokenTree};
use quote::{quote, ToTokens};
use syn::{
    buffer::{Cursor, TokenBuffer},
    Ident, Result,
};

mod kw {
    syn::custom_keyword!(STAGE);
    syn::custom_keyword!(INPUT);
    syn::custom_keyword!(OUTPUT);
    syn::custom_keyword!(ORDERED);
    syn::custom_keyword!(REPLICATE);
}

#[derive(Debug, Clone)]
pub struct VarType(pub TokenStream);

impl PartialEq for VarType {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_string() == other.0.to_string()
    }
}

impl ToTokens for VarType {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.extend(self.0.clone())
    }
}

#[derive(Debug, Clone)]
pub struct SparVar {
    pub identifier: Ident,
    pub var_type: VarType,
}

impl SparVar {
    pub fn new(identifier: Ident, var_type: VarType) -> Self {
        Self {
            identifier,
            var_type,
        }
    }

    pub fn is_vec(&self) -> bool {
        self.var_type.0.to_string().starts_with("Vec")
    }
}

impl PartialEq for SparVar {
    fn eq(&self, other: &Self) -> bool {
        self.identifier == other.identifier
            && self.var_type.0.to_string() == other.var_type.0.to_string()
    }
}

impl ToTokens for SparVar {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Self {
            identifier,
            var_type,
        } = self;
        tokens.extend(quote! {
            #identifier: #var_type
        });
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Replicate {
    Lit(NonZeroU32),
    Var(Ident),
    SeqOrdered,
    SeqUnordered,
}

impl Replicate {
    pub fn is_sequential(&self) -> bool {
        matches!(self, Self::SeqOrdered | Self::SeqUnordered)
    }

    #[allow(dead_code)]
    pub fn is_replicate(&self) -> bool {
        matches!(self, Self::Lit(_) | Self::Var(_))
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct SparAttrs {
    pub input: Vec<SparVar>,
    pub output: Vec<SparVar>,
    pub replicate: Replicate,
}

impl SparAttrs {
    pub fn new(input: Vec<SparVar>, output: Vec<SparVar>, replicate: Replicate) -> Self {
        Self {
            input,
            output,
            replicate,
        }
    }
}

#[derive(Debug)]
pub struct SparStage {
    pub attrs: SparAttrs,
    pub state: Vec<SparVar>,
    pub code: TokenStream,
    pub id: u32,
}

impl SparStage {
    pub fn new(attrs: SparAttrs, code: TokenStream, id: u32) -> Self {
        Self {
            attrs,
            state: Vec::new(),
            code,
            id,
        }
    }
}

impl PartialEq for SparStage {
    fn eq(&self, other: &Self) -> bool {
        self.attrs == other.attrs && self.code.to_string() == other.code.to_string()
    }
}

pub struct SparStream {
    pub attrs: SparAttrs,
    pub stages: Vec<SparStage>,
    pub external_vars: Vec<SparVar>,
}

impl SparStream {
    #[allow(dead_code)]
    pub fn is_external(&self, var: &SparVar) -> bool {
        self.external_vars.contains(var)
    }
}

impl TryFrom<&proc_macro::TokenStream> for SparStream {
    type Error = syn::Error;

    fn try_from(value: &proc_macro::TokenStream) -> std::result::Result<Self, Self::Error> {
        let input = TokenBuffer::new(
            TokenTree::Group(Group::new(Delimiter::Parenthesis, value.clone().into()))
                .into_token_stream()
                .into(),
        );
        let (mut attrs, _, block) = parse_spar_args(input.begin())?;
        let (mut stages, code) = parse_spar_stages(block)?;

        // if there is any code before the stages, it becomes the first stage:
        if !code.is_empty() {
            let mut stage = SparStage::new(attrs.clone(), code.clone(), 0);
            if let Some(s) = stages.get(0) {
                stage.attrs.output = find_variables_in_code(code, &s.attrs.input)?;
            }
            stages.insert(0, stage)
        }

        // any input that was not send by the previous stage becomes 'state'
        for i in 0..stages.len() - 1 {
            if let Some(&mut [ref mut prev, ref mut cur]) = stages.get_mut(i..i + 2) {
                cur.state = cur
                    .attrs
                    .input
                    .iter()
                    .filter_map(|var| {
                        if !prev.attrs.output.contains(var) {
                            Some(var.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                cur.attrs.input.retain(|var| !cur.state.contains(var));
            }
        }

        // variables that exist outside the stream, and that we MAY have to restore later
        let mut external_vars: Vec<SparVar> = attrs.input.clone();
        for stage in &stages {
            for input in &stage.state {
                if !attrs.input.contains(input) {
                    return Err(syn::Error::new(Span::call_site(), "every stage input must either be sent from the previous stage, or be a stream input"));
                }

                if !external_vars.contains(input) {
                    external_vars.push(input.clone());
                }
            }
        }

        // variables that must be restored become the stream's output
        if let Some(stage) = stages.last_mut() {
            for var in &stage.state {
                if external_vars.contains(var) && var.is_vec() {
                    stage.attrs.output.push(var.clone());
                    attrs.output.push(var.clone());
                }
            }
        }

        Ok(Self {
            attrs,
            stages,
            external_vars,
        })
    }
}

fn find_variables_in_code(tokens: TokenStream, to_find: &[SparVar]) -> Result<Vec<SparVar>> {
    let mut vars = Vec::new();

    let buf = TokenBuffer::new2(tokens);
    let mut cursor = buf.begin();

    while let Some((token_tree, mut next)) = cursor.token_tree() {
        match token_tree {
            TokenTree::Group(group) => {
                let inner_vars = find_variables_in_code(group.stream(), to_find)?;
                vars.extend(inner_vars);
            }
            TokenTree::Ident(ident) if ident == "let" => {
                //TODO: TUPLES!!!
                while let Some((token, after)) = next.token_tree() {
                    if let TokenTree::Ident(ident) = token {
                        if ident == "mut" {
                            next = after;
                            continue;
                        }
                    }
                    break;
                }
                if let Some((token, after)) = next.token_tree() {
                    if let TokenTree::Ident(ident) = token {
                        if let Some(v) = to_find.iter().find(|var| var.identifier == ident) {
                            if !vars.contains(v) {
                                vars.push(v.clone());
                            }
                        }
                    }
                    next = after;
                }
            }
            _ => (),
        };

        cursor = next;
    }

    Ok(vars)
}

fn get_type(cursor: Cursor) -> Result<(TokenStream, Cursor)> {
    let mut next = cursor;
    let mut code = TokenStream::new();
    while let Some((token_tree, rest)) = next.token_tree() {
        if let TokenTree::Punct(ref punct) = token_tree {
            if punct.as_char() == ',' {
                return Ok((code, next));
            }
        }
        code.extend(token_tree.into_token_stream());
        next = rest;
    }
    if code.is_empty() {
        Err(syn::Error::new(next.span(), "expected type, found EOF"))
    } else {
        Ok((code, next))
    }
}

/// returns (arguments inside, after parenthesis)
fn skip_parenthesis(cursor: Cursor) -> Result<(Cursor, Cursor)> {
    match cursor.group(Delimiter::Parenthesis) {
        Some((a, _, r)) => Ok((a, r)),
        None => {
            let msg = match cursor.token_tree() {
                Some((tt, _)) => format!("expected arguments, in parenthesis '()', found: {tt}"),
                None => "expected arguments, in parenthesis '()', found: nothing".to_owned(),
            };
            Err(syn::Error::new(cursor.span(), msg))
        }
    }
}

fn get_variables(cursor: Cursor) -> Result<(Vec<SparVar>, Cursor)> {
    let (args, after) = skip_parenthesis(cursor)?;
    let mut rest = args;
    let mut vars = Vec::new();
    while let Some((token_tree, next)) = rest.token_tree() {
        match &token_tree {
            TokenTree::Ident(identifier) => {
                let next = skip_punct(next, ':')?;
                let (var_type, next) = get_type(next)?;
                vars.push(SparVar::new(identifier.clone(), VarType(var_type)));
                match skip_punct(next, ',') {
                    Ok(next) => rest = next,
                    Err(e) => {
                        if next.token_tree().is_none() {
                            break;
                        } else {
                            return Err(e);
                        }
                    }
                }
            }

            _ => {
                let msg = format!("unexpected token '{token_tree}'");
                return Err(syn::Error::new(rest.span(), msg));
            }
        }
    }
    Ok((vars, after))
}

fn parse_replicate(cursor: Cursor) -> Result<(Replicate, Cursor)> {
    if let Some((TokenTree::Punct(punct), next)) = cursor.token_tree() {
        if punct.as_char() == '=' {
            if let Some((tt, next)) = next.token_tree() {
                match tt {
                    TokenTree::Ident(i) => return Ok((Replicate::Var(i), next)),
                    TokenTree::Literal(lit) => {
                        if let Ok(i) = lit.to_string().parse::<u32>() {
                            if i > 0 {
                                return Ok((Replicate::Lit(NonZeroU32::new(i).unwrap()), next));
                            } else {
                                return Err(syn::Error::new(
                                    cursor.span(),
                                    "'REPLICATE' cannot have an argument of '0'",
                                ));
                            }
                        }
                    }
                    _ => (),
                }
            }
        }
    }

    Err(syn::Error::new(
        cursor.span(),
        "failed to parse REPLICATE attribute.
                        Correct syntax is: 'REPLICATE = N', where is in either a
                        number or an identifier",
    ))
}

fn skip_punct(cursor: Cursor, punct: char) -> Result<Cursor> {
    if let Some((token_tree, next)) = cursor.token_tree() {
        if let TokenTree::Punct(ref p) = token_tree {
            if p.as_char() == punct {
                return Ok(next);
            }
        }
        let msg = format!("expected '{punct}', found '{token_tree}'");
        return Err(syn::Error::new(next.span(), msg));
    }
    Err(syn::Error::new(
        cursor.span(),
        "expected '{punct}', found EOF",
    ))
}

fn parse_spar_args(cursor: Cursor) -> Result<(SparAttrs, Cursor, Cursor)> {
    let (args, after) = skip_parenthesis(cursor)?;

    let mut input: Vec<SparVar> = Vec::new();
    let mut output: Vec<SparVar> = Vec::new();
    let mut replicate = Replicate::SeqUnordered;

    let mut rest = args;
    while let Some((token_tree, next)) = rest.token_tree() {
        match &token_tree {
            TokenTree::Ident(ident) => match ident.to_string().as_str() {
                "INPUT" => {
                    if !input.is_empty() {
                        return Err(syn::Error::new(
                            rest.span(),
                            "multiple INPUTs aren't allowed",
                        ));
                    }
                    let (i, next) = get_variables(next)?;
                    if i.is_empty() {
                        return Err(syn::Error::new(rest.span(), "INPUT cannot be empty"));
                    }
                    input = i;
                    rest = skip_punct(next, ',')?;
                }
                "OUTPUT" => {
                    if !output.is_empty() {
                        return Err(syn::Error::new(
                            rest.span(),
                            "multiple OUTPUTs aren't allowed",
                        ));
                    }
                    let (o, next) = get_variables(next)?;
                    if o.is_empty() {
                        return Err(syn::Error::new(rest.span(), "INPUT cannot be empty"));
                    }
                    output = o;
                    rest = skip_punct(next, ',')?;
                }
                "REPLICATE" => {
                    if !matches!(replicate, Replicate::SeqUnordered) {
                        return Err(syn::Error::new(
                            rest.span(),
                            "multiple REPLICATEs aren't allowed",
                        ));
                    }
                    let (r, next) = parse_replicate(next)?;
                    replicate = r;
                    rest = skip_punct(next, ',')?;
                }
                "ORDERED" => {
                    if !matches!(replicate, Replicate::SeqUnordered) {
                        return Err(syn::Error::new(
                            rest.span(),
                            "only one of REPLICATE or ORDERED can be specified",
                        ));
                    }
                    replicate = Replicate::SeqOrdered;
                    rest = skip_punct(next, ',')?;
                }

                _ => {
                    let msg = std::format!( "unexpected token '{token_tree}'. Valid tokens are 'INPUT(args)', 'OUTPUT(args)', 'REPLICATE = N' and a code block");
                    return Err(syn::Error::new(rest.span(), msg));
                }
            },

            TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => {
                let (group_cursor, _, next) = rest.group(group.delimiter()).unwrap();
                if next.token_tree().is_some() && skip_punct(next, ',').is_err() {
                    return Err(syn::Error::new(
                        next.span(),
                        "unexpected token after code block",
                    ));
                }
                return Ok((
                    SparAttrs::new(input, output, replicate),
                    after,
                    group_cursor,
                ));
            }

            _ => {
                let msg = std::format!( "unexpected token '{token_tree}'. Valid tokens are 'INPUT(args)', 'OUTPUT(args)', 'REPLICATE = N' and a code block");
                return Err(syn::Error::new(rest.span(), msg));
            }
        }
    }

    Err(syn::Error::new(
        rest.span(),
        "expected a '{...}' code block",
    ))
}

fn parse_spar_stages(cursor: Cursor) -> Result<(Vec<SparStage>, TokenStream)> {
    let mut stages = Vec::new();
    let mut code_stack = vec![TokenStream::new()];

    let mut groups = Vec::new();
    let mut rest = cursor;
    loop {
        while let Some((token_tree, next)) = rest.token_tree() {
            match &token_tree {
                TokenTree::Ident(ident) if ident == "STAGE" => {
                    groups.clear();
                    if !code_stack.iter().all(|code| code.is_empty()) {
                        code_stack
                            .last_mut()
                            .unwrap()
                            .extend(quote! {__SPAR_MARKER__});
                    }

                    while code_stack.len() > 1 {
                        let code = code_stack.pop().unwrap();
                        code_stack.last_mut().unwrap().extend(
                            TokenTree::Group(Group::new(Delimiter::Brace, code))
                                .into_token_stream(),
                        );
                    }
                    break;
                }

                TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => {
                    let (group_cursor, _, next) = rest.group(group.delimiter()).unwrap();
                    code_stack.push(TokenStream::new());
                    rest = group_cursor;
                    groups.push(next);
                }

                _ => {
                    token_tree.to_tokens(code_stack.last_mut().unwrap());
                    rest = next;
                }
            }
        }
        if let Some(cursor) = groups.pop() {
            rest = cursor;
            let code = code_stack.pop().unwrap();
            code_stack
                .last_mut()
                .unwrap()
                .extend(TokenTree::Group(Group::new(Delimiter::Brace, code)).into_token_stream());
        } else {
            break;
        }
    }

    while let Some((token_tree, next)) = rest.token_tree() {
        match &token_tree {
            TokenTree::Ident(ident) if *ident == "STAGE" => {
                let (attrs, semicolon, code_cursor) = parse_spar_args(next)?;
                stages.push(SparStage::new(
                    attrs,
                    code_cursor.token_stream(),
                    stages.len() as u32 + 1,
                ));

                match semicolon.token_tree() {
                    Some((token, next)) => match token {
                        TokenTree::Punct(punct) if punct.as_char() == ';' => {
                            rest = next;
                        }
                        _ => return Err(syn::Error::new(next.span(), "expected ';'")),
                    },
                    None => return Err(syn::Error::new(next.span(), "expected ';'")),
                }
            }

            _ => {
                return Err(syn::Error::new(
                    next.span(),
                    format!("expected 'STAGE' or END OF STREAM, found {token_tree}"),
                ))
            }
        }
    }

    Ok((stages, code_stack.pop().unwrap()))
}

#[cfg(test)]
mod tests {
    use proc_macro2::{Span, TokenStream};
    use quote::quote;

    use super::*;

    /// Returns the span of the first instance of an identifier with the same name
    fn get_ident_span(identifier: &'static str, tokens: TokenStream) -> Option<Span> {
        let buffer = TokenBuffer::new2(tokens);
        let cursor = buffer.begin();

        let mut groups = vec![cursor];
        while !groups.is_empty() {
            let mut rest = groups.pop().unwrap();
            while let Some((token_tree, next)) = rest.token_tree() {
                match &token_tree {
                    TokenTree::Ident(ident) if *ident == identifier => {
                        return Some(ident.span());
                    }

                    TokenTree::Group(group) => {
                        let (group_cursor, _, next) = rest.group(group.delimiter()).unwrap();
                        groups.push(next);
                        rest = group_cursor;
                    }

                    _ => rest = next,
                }
            }
        }

        None
    }

    fn make_vars(
        idents: &[&'static str],
        types: &[&'static str],
        tokens: &TokenStream,
    ) -> Vec<SparVar> {
        if idents.len() != types.len() {
            panic!("must have the same number of idents and types");
        }
        let mut vec = Vec::new();

        for (i, ident) in idents.iter().enumerate() {
            let span =
                get_ident_span(ident, tokens.clone()).expect("Failed to find identifier in stream");
            vec.push(SparVar::new(
                Ident::new(ident, span),
                VarType(Ident::new(types[i], span).to_token_stream()),
            ));
        }

        vec
    }

    #[test]
    fn top_level_code() {
        let stage = quote! {
            let mut a = 10;
            while true {
                a += 1;
                STAGE({
                    let b = 10;
                });
            }
        };

        let expected_tokens = quote! {
            let mut a = 10;
            while true {
                a += 1;
                __SPAR_MARKER__
            }
        };

        let (_, tokens) = parse_spar_stages(TokenBuffer::new2(stage).begin()).unwrap();
        assert_eq!(expected_tokens.to_string(), tokens.to_string());
    }

    #[test]
    fn stage_no_attributes() {
        let tokens = quote! {
            // Put some dummy code just to make sure nothing will break
            let mut a = 10;
            while true {
                a += 1;
            }
        };
        let stage = quote! {
            STAGE({
                #tokens
            });
        };

        let (mut spar_stages, _) = parse_spar_stages(TokenBuffer::new2(stage).begin()).unwrap();
        assert_eq!(spar_stages.len(), 1);

        let expected_attrs = SparAttrs::new(Vec::new(), Vec::new(), Replicate::SeqUnordered);
        assert_eq!(
            spar_stages.pop().unwrap(),
            SparStage::new(expected_attrs, tokens, 0)
        );
    }

    #[test]
    fn stage_with_input() {
        let tokens = quote! {
            while true {
                a += 1;
            }
        };
        let stage = quote! {
            STAGE(INPUT(a: u32), {
                #tokens
            });
        };

        let (mut spar_stages, _) =
            parse_spar_stages(TokenBuffer::new2(stage.clone()).begin()).unwrap();
        assert_eq!(spar_stages.len(), 1);

        let input = make_vars(&["a"], &["u32"], &stage);
        let output = Vec::new();
        let replicate = Replicate::SeqUnordered;
        let expected_attrs = SparAttrs::new(input, output, replicate);
        assert_eq!(
            spar_stages.pop().unwrap(),
            SparStage::new(expected_attrs, tokens, 0)
        );
    }

    #[test]
    fn stage_with_multiple_inputs() {
        let tokens = quote! {
            while true {
                a += 1;
                b += 2,
                c += 3;
            }
        };
        let stage = quote! {
            STAGE(INPUT(a: u32, b: u32, c: u32), {
                #tokens
            });
        };

        let (mut spar_stages, _) =
            parse_spar_stages(TokenBuffer::new2(stage.clone()).begin()).unwrap();
        assert_eq!(spar_stages.len(), 1);

        let input = make_vars(&["a", "b", "c"], &["u32", "u32", "u32"], &stage);
        let output = Vec::new();
        let replicate = Replicate::SeqUnordered;
        let expected_attrs = SparAttrs::new(input, output, replicate);
        assert_eq!(
            spar_stages.pop().unwrap(),
            SparStage::new(expected_attrs, tokens, 0)
        );
    }

    #[test]
    fn stage_with_output() {
        let tokens = quote! {
                while true {
                    a += 1;
                }
        };
        let stage = quote! {
            STAGE(OUTPUT(a: u32), {
                #tokens
            });
        };

        let (mut spar_stages, _) =
            parse_spar_stages(TokenBuffer::new2(stage.clone()).begin()).unwrap();
        assert_eq!(spar_stages.len(), 1);

        let input = vec![];
        let output = make_vars(&["a"], &["u32"], &stage);
        let replicate = Replicate::SeqUnordered;
        let expected_attrs = SparAttrs::new(input, output, replicate);
        assert_eq!(
            spar_stages.pop().unwrap(),
            SparStage::new(expected_attrs, tokens, 0)
        );
    }

    #[test]
    fn stage_with_multiple_outputs() {
        let tokens = quote! {
            while true {
                a += 1;
                b += 2,
                c += 3;
            }
        };

        let stage = quote! {
            STAGE(OUTPUT(a: u32,b: u32, c: u32), {
                #tokens
            });
        };

        let (mut spar_stages, _) =
            parse_spar_stages(TokenBuffer::new2(stage.clone()).begin()).unwrap();
        assert_eq!(spar_stages.len(), 1);

        let input = vec![];
        let output = make_vars(&["a", "b", "c"], &["u32", "u32", "u32"], &stage);
        let replicate = Replicate::SeqUnordered;
        let expected_attrs = SparAttrs::new(input, output, replicate);
        assert_eq!(
            spar_stages.pop().unwrap(),
            SparStage::new(expected_attrs, tokens, 0)
        );
    }

    #[test]
    fn stage_with_replicate() {
        let tokens = quote! {
            let mut a = 10;
            while true {
                a += 1;
            }
        };
        let stage = quote! {
            STAGE(REPLICATE = 5, {
                #tokens
            });
        };

        let (mut spar_stages, _) = parse_spar_stages(TokenBuffer::new2(stage).begin()).unwrap();
        assert_eq!(spar_stages.len(), 1);

        let expected_attrs = SparAttrs::new(
            Vec::new(),
            Vec::new(),
            Replicate::Lit(NonZeroU32::new(5).unwrap()),
        );
        assert_eq!(
            spar_stages.pop().unwrap(),
            SparStage::new(expected_attrs, tokens, 0)
        );
    }

    #[test]
    fn multiple_stages() {
        let stage = quote! {
            STAGE({});
            STAGE(INPUT(a: u32), OUTPUT(b: u32), {});
            STAGE(INPUT(c: u32, d: u32), OUTPUT(e: u32, f: u32, g: u32), {});
            STAGE(INPUT(h: u32), OUTPUT(i: u32), REPLICATE = 5, {});
        };

        let (mut spar_stages, _) =
            parse_spar_stages(TokenBuffer::new2(stage.clone()).begin()).unwrap();
        assert_eq!(spar_stages.len(), 4);
        spar_stages.reverse();

        let expected_attrs = SparAttrs::new(Vec::new(), Vec::new(), Replicate::SeqUnordered);
        assert_eq!(
            spar_stages.pop().unwrap(),
            SparStage::new(expected_attrs, TokenStream::new(), 0)
        );

        let input = make_vars(&["a"], &["u32"], &stage);
        let output = make_vars(&["b"], &["u32"], &stage);
        let replicate = Replicate::SeqUnordered;
        let expected_attrs = SparAttrs::new(input, output, replicate);
        assert_eq!(
            spar_stages.pop().unwrap(),
            SparStage::new(expected_attrs, TokenStream::new(), 0)
        );

        let input = make_vars(&["c", "d"], &["u32", "u32"], &stage);
        let output = make_vars(&["e", "f", "g"], &["u32", "u32", "u32"], &stage);
        let replicate = Replicate::SeqUnordered;
        let expected_attrs = SparAttrs::new(input, output, replicate);
        assert_eq!(
            spar_stages.pop().unwrap(),
            SparStage::new(expected_attrs, TokenStream::new(), 0)
        );

        let input = make_vars(&["h"], &["u32"], &stage);
        let output = make_vars(&["i"], &["u32"], &stage);
        let replicate = Replicate::Lit(NonZeroU32::new(5).unwrap());
        let expected_attrs = SparAttrs::new(input, output, replicate);
        assert_eq!(
            spar_stages.pop().unwrap(),
            SparStage::new(expected_attrs, TokenStream::new(), 0)
        );
    }

    #[test]
    fn complex_type() {
        let complex_type = quote! { Vec<(u32, &'a mut u32)> };
        let stage = quote! {
            STAGE(INPUT(a: #complex_type), {});
        };

        let (spar_stages, _) = parse_spar_stages(TokenBuffer::new2(stage).begin()).unwrap();
        assert_eq!(
            spar_stages[0].attrs.input[0].var_type.0.to_string(),
            complex_type.to_string()
        );
    }

    #[test]
    #[should_panic]
    fn input_cannot_be_a_literal() {
        let stage = quote! {
            STAGE(INPUT(10), {});
        };

        let _spar_stages = parse_spar_stages(TokenBuffer::new2(stage).begin()).unwrap();
    }

    #[test]
    #[should_panic]
    fn input_cannot_be_empty() {
        let stage = quote! {
            STAGE(INPUT(), {});
        };

        let _spar_stages = parse_spar_stages(TokenBuffer::new2(stage).begin()).unwrap();
    }

    #[test]
    #[should_panic]
    fn output_cannot_be_empty() {
        let stage = quote! {
            STAGE(OUTPUT(), {});
        };

        let _spar_stages = parse_spar_stages(TokenBuffer::new2(stage).begin()).unwrap();
    }

    #[test]
    #[should_panic]
    fn forgot_comma() {
        let stage = quote! {
            STAGE(REPLICATE = 4 {});
        };

        let _spar_stages = parse_spar_stages(TokenBuffer::new2(stage).begin()).unwrap();
    }

    #[test]
    #[should_panic]
    fn should_not_have_code_after_stages() {
        let stage = quote! {
            STAGE(REPLICATE = 4 {});
            let a = 5;
        };

        let _spar_stages = parse_spar_stages(TokenBuffer::new2(stage).begin()).unwrap();
    }
}
