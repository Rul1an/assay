# Parser clarification after final review

This supplemental contract addresses F2/F3 of the review of a0c91c4. It does not
change the frozen CONTRACT.md, input inventory, or 19 pre-reader expectations.
Supplemental regression cases are repairs prompted by known findings, not blind evidence.

Integer JSON tokens may contain at most 4300 decimal digits, excluding the optional
minus sign. This limit applies anywhere, including uninterpreted unknown fields,
and is independent of Python host int-string settings. Conversion is exact;
protected amounts still require integer syntax and the original 1..2^63-1 range.

Fraction/exponent JSON tokens must convert to a finite IEEE-754 binary64 value.
Overflow refuses the entire input with exit 2 and no stdout; underflow to finite
zero is allowed in unknown fields. Such tokens remain non-integers even if their
value is mathematically integral. No float conversion is used for money comparison.

Escaped UTF-16 surrogate pairs denote the same scalar as the equivalent literal
UTF-8 character for duplicate-object-key detection at every depth.
