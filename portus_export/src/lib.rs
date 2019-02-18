extern crate proc_macro;
use proc_macro::TokenStream;
use quote::quote;

#[proc_macro_attribute]
pub fn register_ccp_alg(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let alg_struct = syn::parse_macro_input!(item as syn::ItemStruct);
    let alg_struct_name = &alg_struct.ident;
    let result = quote! {
        #alg_struct
        pub type __ccp_alg_export = #alg_struct_name;
    };
    result.into()
}
