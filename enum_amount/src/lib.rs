use proc_macro::TokenStream;

use syn::{parse_macro_input, DeriveInput};

use quote::quote;


#[proc_macro_derive(EnumCount)]
pub fn enum_count(input: TokenStream) -> TokenStream
{
    let input = parse_macro_input!(input as DeriveInput);

    let variants = match input.data
    {
        syn::Data::Enum(ref item) =>
        {
            &item.variants
        },
        _ => panic!("EnumCount cannot be derived on non enums")
    };

    let enum_name = input.ident;

    let identifiers = variants.iter().map(|variant|
    {
        &variant.ident
    });

    let index_mappings = identifiers.enumerate().map(|(index, identifier)|
    {
        quote!
        {
            #enum_name::#identifier{..} => #index,
        }
    }).collect::<Vec<_>>();

    //this wont work with generics im pretty sure
    let count = variants.len();
    let expanded = quote!
    {
        pub const COUNT: usize = #count;

        impl #enum_name
        {
            pub fn index(&self) -> usize
            {
                match self
                {
                    #(#index_mappings)*
                }
            }
        }
    };

    expanded.into()
}