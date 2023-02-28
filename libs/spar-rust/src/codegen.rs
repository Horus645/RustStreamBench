use crate::spar_stream::{Replicate, SparStage, SparStream, SparVar, VarType};
use proc_macro2::{Delimiter, Group, Ident, Span, TokenStream, TokenTree};
use quote::{quote, ToTokens};

struct Dispatcher {
    code: TokenStream,
}

impl Dispatcher {
    fn copy_code(tokens: TokenTree, found: &mut bool, replacement: &TokenStream) -> TokenStream {
        match tokens {
            TokenTree::Group(group) => Group::new(
                group.delimiter(),
                group
                    .stream()
                    .into_iter()
                    .map(|token| Self::copy_code(token, found, replacement))
                    .collect(),
            )
            .into_token_stream(),
            TokenTree::Ident(ident) => {
                if ident == "__SPAR_MARKER__" {
                    *found = true;
                    replacement.clone()
                } else {
                    ident.into_token_stream()
                }
            }
            TokenTree::Punct(punct) => punct.into_token_stream(),
            TokenTree::Literal(literal) => literal.into_token_stream(),
        }
    }

    pub fn new(stage: &SparStage, next_stage: Option<&SparStage>) -> (Self, bool) {
        let mut idents = Vec::new();
        if let Some(next_stage) = next_stage {
            for input in &next_stage.attrs.input {
                idents.push(input.identifier.clone())
            }
        } else {
            for input in &stage.attrs.input {
                idents.push(input.identifier.clone())
            }
        }
        let inputs = make_tuple(&idents);

        let pipeline_post = quote! { spar_pipeline.post(#inputs).unwrap(); };
        let mut gen = TokenStream::new();
        let mut found = false;
        for token in stage.code.clone().into_iter() {
            gen.extend(Self::copy_code(token, &mut found, &pipeline_post));
        }

        if found {
            (Self { code: gen }, true)
        } else {
            (
                Self {
                    code: pipeline_post,
                },
                false,
            )
        }
    }
}

impl ToTokens for Dispatcher {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.extend(self.code.clone());
    }
}

///Note: replicate defaults to 1 when it is not given.
///If REPLICATE argument exists, then it defaults to what was written in the code
///if SPAR_NUM_WORKERS is set, all REPLICATES are set to that value
fn gen_replicate(replicate: &Replicate) -> TokenStream {
    //NOTE: this needs to be i32 in rust_spp
    match replicate {
        Replicate::Var(v) => {
            quote!(#v as i32)
        }

        Replicate::Lit(n) => {
            let n: u32 = (*n).into();
            quote! {
                if let Some(workers) = spar_num_workers {
                    workers as i32
                } else {
                    #n as i32
                }
            }
        }
        Replicate::None => quote!(1),
    }
}

fn gen_spar_num_workers() -> TokenStream {
    quote! {
        // Set spar_num_workers according to the envvar SPAR_NUM_WORKERS
        // If it doesn't exist, OR it is invalid, we simply set it to NONE
        let spar_num_workers: Option<u32> = match std::env::var("SPAR_NUM_WORKERS") {
            Ok(var) => match var.parse() {
                Ok(value) => if value < 1 {
                    eprintln!("SPAR_NUM_WORKERS must be a number > 0. Found {}. Defaulting to 1...", value);
                    Some(1)
                } else {
                    Some(value)
                },
                Err(_) => {
                    eprintln!("invalid value for SPAR_NUM_WORKERS variable: {}. Ignoring...", var);
                    None
                }
            }
            Err(_) => None
        };
    }
}

fn make_tuple<T: ToTokens>(tokens: &[T]) -> TokenStream {
    quote! { ( #(#tokens),* ) }
}

fn make_mut_tuple<T: ToTokens>(tokens: &[T]) -> TokenStream {
    quote! { ( #(mut #tokens),* ) }
}

fn get_idents_and_types_from_spar_vars(vars: &[SparVar]) -> (Vec<Ident>, Vec<VarType>) {
    let mut idents = Vec::new();
    let mut types = Vec::new();

    for var in vars {
        idents.push(var.identifier.clone());
        types.push(var.var_type.clone());
    }

    (idents, types)
}

fn rust_spp_stage_struct_gen(stage: &SparStage) -> TokenStream {
    let (in_idents, in_types) = get_idents_and_types_from_spar_vars(&stage.attrs.input);
    let out_types = &stage.attrs.output;

    let struct_name = format!("SparStage{}", stage.id);
    let struct_ident = Ident::new(&struct_name, Span::call_site());
    let stage_code = &stage.code;

    let mut code = quote! {
        struct #struct_ident {
            // TODO: we need to declare variables for stateful computation
        }

        impl #struct_ident {
            fn new() -> Self {
                Self {}
            }
        }
    };

    if !in_types.is_empty() && !out_types.0.is_empty() {
        let in_types = make_tuple(&in_types);

        let input_tuple = make_mut_tuple(&in_idents);

        code.extend(quote! {
            impl rust_spp::blocks::inout_block::InOut<#in_types, #out_types> for #struct_ident {
                fn process(&mut self, input: #in_types) -> Option<#out_types> {
                    let #input_tuple = input;
                    #stage_code
                }
            }
        });
    } else if !in_types.is_empty() {
        let in_types = make_tuple(&in_types);
        let input_tuple = make_mut_tuple(&in_idents);
        code.extend(quote! {
            impl rust_spp::blocks::in_block::In<#in_types> for #struct_ident {
                fn process(&mut self, input: #in_types, order: u64) {
                    let #input_tuple = input;
                    #stage_code
                }
            }
        });
    } else {
        //ERROR
        panic!("ERROR: Stage without input is invalid!");
    }

    code
}

fn rust_spp_gen_top_level_code(spar_stream: &mut SparStream) -> (Vec<TokenStream>, Dispatcher) {
    let SparStream { ref mut stages, .. } = spar_stream;
    let mut structs = Vec::new();

    let (dispatcher, found) = Dispatcher::new(&stages[0], stages.get(1));
    if found {
        stages.remove(0);
    }

    for stage in stages {
        structs.push(rust_spp_stage_struct_gen(stage));
    }

    (structs, dispatcher)
}

fn rust_spp_pipeline_arg(stage: &SparStage) -> TokenStream {
    let SparStage { attrs, id, .. } = stage;
    let struct_name = format!("SparStage{id}");
    let struct_ident = Ident::new(&struct_name, Span::call_site());

    if attrs.replicate.is_none() {
        quote! { rust_spp::sequential!(#struct_ident::new()) }
    } else {
        let replicate = gen_replicate(&attrs.replicate);
        quote! { rust_spp::parallel!(#struct_ident::new(), #replicate) }
    }
}

fn rust_spp_gen(spar_stream: &mut SparStream) -> TokenStream {
    let (spar_structs, dispatcher) = rust_spp_gen_top_level_code(spar_stream);
    let mut gen = TokenStream::new();

    let mut code = quote! {
        use rust_spp::*;
    };
    for (stage, spar_struct) in spar_stream.stages.iter().zip(spar_structs) {
        code.extend(spar_struct);

        if !gen.is_empty() {
            gen.extend(quote!(,));
        }

        gen.extend(rust_spp_pipeline_arg(stage));
    }

    code.extend(quote! {
        let spar_pipeline = rust_spp::pipeline![
            #gen,
            collect_ordered!()
        ];

        #dispatcher
        spar_pipeline.collect()
    });

    code
}

pub fn codegen(mut spar_stream: SparStream) -> TokenStream {
    let mut code = gen_spar_num_workers();
    code.extend(rust_spp_gen(&mut spar_stream));

    Group::new(Delimiter::Brace, code).into_token_stream()
}

//TODO: test the code generation, once we figure it out
//#[cfg(test)]
//mod tests {
//    use super::*;
//
//    #[test]
//    fn should_() {
//
//    }
//}
