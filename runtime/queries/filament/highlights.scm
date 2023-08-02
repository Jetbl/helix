[
  "comp" 
  "extern" 
  "where" 
  "import" 
  "bundle" 
  "new" 
  "in" 
] @keyword

(interface "@interface" @keyword)

(fact "assume" @keyword)
(fact "assert" @keyword)

[
  "for"
] @keyword.control


[
  "if"
  "else"
] @keyword.control.conditional

(signature (identifier) @constructor)

(instance
 "new"
 (identifier) @constructor)

(instance
 (identifier) @varible)

(bundle
  "bundle"
 (identifier) @varible)

(port
 (identifier) @varible)

(port_def
  (identifier) @variable)

(param_var) @constant.numeric

(time (identifier) @variable.parameter)
(interface (identifier) @variable.parameter)
(event_with_delay (identifier) @variable.parameter)

(comment) @comment

(string_lit) @string

(bitwidth) @constant.numeric  

[
  (order_op)
  "%"
  "-"
  "+"
  "/"
  "*"
] @keyword.operator

[
 ";"
 ","
] @punctuation.delimiter


[
  "["
  "]"
  "("
  ")"
] @punctuation.bracket