use crate::spar_stream::{Replicate, SparStage, SparStream, SparVar, VarType};
use proc_macro2::{Group, Ident, Span, TokenStream, TokenTree};
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
    let (out_idents, out_types) = get_idents_and_types_from_spar_vars(&stage.attrs.output);

    let struct_name = format!("SparStage{}", stage.id);
    let struct_ident = Ident::new(&struct_name, Span::call_site());
    let stage_code = &stage.code;
    let state = &stage.state;
    let state_idents: TokenStream = state
        .iter()
        .flat_map(|var| {
            let ident = &var.identifier;
            quote! { #ident, }
        })
        .collect();

    let mut code = quote! {
        struct #struct_ident {
            #(#state),*
        }

        impl #struct_ident {
            fn new(#(#state),*) -> Self {
                Self { #state_idents }
            }
        }
    };

    let state_deconstruct: TokenStream = state
        .iter()
        .flat_map(|var| {
            let ident = &var.identifier;
            if stage.attrs.output.contains(var) {
                quote! {
                    let mut #ident = self.#ident.clone();
                }
            } else {
                quote! {
                    let #ident = &mut self.#ident;
                }
            }
        })
        .collect();

    if !in_types.is_empty() && !out_types.is_empty() {
        let in_types = make_tuple(&in_types);
        let out_types = make_tuple(&out_types);

        let input_tuple = make_mut_tuple(&in_idents);
        let output_tuple = make_tuple(&out_idents);

        code.extend(quote! {
            impl rust_spp::blocks::inout_block::InOut<#in_types, #out_types> for #struct_ident {
                fn process(&mut self, input: #in_types) -> Option<#out_types> {
                    let #input_tuple = input;
                    #state_deconstruct
                    #stage_code
                    Some(#output_tuple)
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
                    #state_deconstruct
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
    let SparStage {
        attrs, id, state, ..
    } = stage;
    let struct_name = format!("SparStage{id}");
    let struct_ident = Ident::new(&struct_name, Span::call_site());

    let struct_new_args: Vec<TokenStream> = state
        .iter()
        .map(|var| {
            let ident = &var.identifier;
            quote! { #ident }
        })
        .collect();

    if attrs.replicate.is_none() {
        quote! { rust_spp::sequential_ordered!(#struct_ident::new( #(#struct_new_args.clone()),* )) }
    } else {
        let replicate = gen_replicate(&attrs.replicate);
        quote! { rust_spp::parallel!(#struct_ident::new( #(#struct_new_args.clone()),* ), #replicate) }
    }
}

fn rust_spp_gen_pipeline(spar_stream: &SparStream, gen: TokenStream) -> TokenStream {
    if let Some(stage) = spar_stream.stages.last() {
        if stage.attrs.replicate == Replicate::None && stage.attrs.output.is_empty() {
            return quote! { let mut spar_pipeline = rust_spp::pipeline![#gen]; };
        }
    }

    quote! {
        let mut spar_pipeline = rust_spp::pipeline![
            #gen,
            collect_ordered!()
        ];
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

    code.extend(rust_spp_gen_pipeline(spar_stream, gen));
    code.extend(quote! {#dispatcher});
    if !spar_stream.attrs.output.is_empty() {
        code.extend(quote! {
            let collection = spar_pipeline.collect();
        })
    } else {
        code.extend(quote! {
            spar_pipeline.end_and_wait();
        })
    }

    code
}

fn restore_external_vars(spar_stream: &SparStream) -> TokenStream {
    let (ident, vtype) = get_idents_and_types_from_spar_vars(&spar_stream.attrs.output);
    let mut code = TokenStream::new();

    for (ident, vtype) in ident.iter().zip(vtype) {
        code.extend(quote! {
            let mut #ident: #vtype = Vec::new();

        });
    }

    if ident.len() > 1 {
        for (i, ident) in ident.iter().enumerate() {
            code.extend(quote! {
                for tuple in collection {
                    #ident.extend(tuple.#i);
                }
            });
        }
    } else if ident.len() == 1 {
        let ident = &ident[0];
        code.extend(quote! {
            for elem in collection {
                #ident.extend(elem);
            }
        })
    }

    code
}

pub fn codegen(mut spar_stream: SparStream) -> TokenStream {
    let mut code = gen_spar_num_workers();
    code.extend(rust_spp_gen(&mut spar_stream));
    code.extend(restore_external_vars(&spar_stream));

    code
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
