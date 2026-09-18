(struct_declaration "struct" @context name: (type_identifier) @name) @item
(enum_declaration "enum" @context name: (type_identifier) @name) @item
(interface_declaration "interface" @context name: (type_identifier) @name) @item
(impl_declaration "impl" @context target: (named_type) @name) @item
(function_declaration "fn" @context name: (identifier) @name) @item
(native_function_declaration "native" @context "fn" @context name: (identifier) @name) @item
(interface_method "fn" @context name: (identifier) @name) @item
(const_declaration "const" @context name: (identifier) @name) @item
