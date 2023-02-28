mod codegen;
mod spar_stream;

use codegen::codegen;
use spar_stream::SparStream;

#[proc_macro]
pub fn to_stream(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    match SparStream::try_from(&item) {
        Ok(spar_stream) => codegen(spar_stream).into(),
        Err(e) => e.into_compile_error().into(),
    }
}
