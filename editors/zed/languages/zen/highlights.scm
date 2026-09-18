(identifier) @variable
(type_identifier) @type
((type_identifier) @type.builtin (#match? @type.builtin "^(Bool|Int|Float|String|Char|Unit|Never|I8|I16|I32|I64|U8|U16|U32|U64|F32|F64)$"))
(function_declaration name: (identifier) @function)
(native_function_declaration name: (identifier) @function)
(interface_method name: (identifier) @function)
(parameter name: (identifier) @variable.parameter)
(struct_field name: (identifier) @property)
(field_initializer name: (identifier) @property)
(member_expression name: (identifier) @property)
(const_declaration name: (identifier) @constant)
(enum_variant name: (identifier) @variant)
(enum_constructor_expression name: (identifier) @variant)
(string_literal) @string
(char_literal) @string
(escape_sequence) @string.escape
[(integer_literal) (float_literal)] @number
(boolean_literal) @boolean
(unit_literal) @constant.builtin
[(line_comment) (block_comment)] @comment
["import" "as" "struct" "enum" "interface" "impl" "fn" "native" "async" "const" "let" "var" "return" "defer" "while" "for" "in" "break" "continue" "if" "else" "match" "await" "with"] @keyword
"self" @variable.special
["+" "-" "*" "!" "?" "=" "==" "!=" "<" ">" "<=" ">=" "&&" "||" "&" "->" "=>"] @operator
["(" ")" "[" "]" "{" "}"] @punctuation.bracket
[";" "," "." ":"] @punctuation.delimiter

(visibility) @keyword
