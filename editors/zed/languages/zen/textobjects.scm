(function_declaration body: (block "{" (_)* @function.inside "}")) @function.around
(native_function_declaration) @function.around
(struct_declaration body: (struct_body "{" (_)* @class.inside "}")) @class.around
(enum_declaration (enum_body "{" (_)* @class.inside "}")) @class.around
(interface_declaration (interface_body "{" (_)* @class.inside "}")) @class.around
(impl_declaration (impl_body "{" (_)* @class.inside "}")) @class.around
(line_comment)+ @comment.around
(block_comment) @comment.around
