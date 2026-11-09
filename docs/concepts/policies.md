# Policies

Policies define what "correct" means for your AI agent's tool usage.

---

## What is a Policy?

A **policy** says which tools an agent may call and what their arguments may be:

- an allow or deny list of tool names;
- a JSON Schema per tool for its argument object: types, required arguments, bounds, patterns,
  allowed values, nested objects and arrays.

When Assay replays a trace, an `args_valid` test checks every tool call against the policy. A call
that breaks it fails the test.

---

## Policy Structure

A policy is a YAML file with `version: "2.0"`, an optional allow or deny list, and one JSON Schema
per tool under `schemas:`:

```yaml
# policies/customer-service.yaml
version: "2.0"

deny: [delete_customer]

schemas:
  get_customer:
    type: object
    properties:
      id:
        type: string
        pattern: "^cust_[0-9]+$"
    required: [id]

  apply_discount:
    type: object
    properties:
      percent:
        type: number
        minimum: 0
        maximum: 30
      order_id:
        type: string
    required: [percent, order_id]

  send_email:
    type: object
    properties:
      to:
        type: string
        pattern: "^[^@\\s]+@[^@\\s]+$"
      subject:
        type: string
        maxLength: 200
    required: [to]
```

Each tool's arguments go under `properties`, and the arguments that must be present under
`required`. A map of argument names directly under the tool name is not a JSON Schema and is
refused as a configuration error, with a message that says so. The full field reference is
[MCP Policy Files](../reference/config/policies.md).

---

## Constraint Types

These are JSON Schema keywords, written inside a tool's `properties`.

### Types

```yaml
properties:
  name:
    type: string
  age:
    type: number
  active:
    type: boolean
  tags:
    type: array
  metadata:
    type: object
```

### Required Arguments

```yaml
type: object
properties:
  id:
    type: string
  nickname:
    type: string
required: [id]   # nickname is optional
```

### Numbers

```yaml
properties:
  quantity:
    type: integer
    minimum: 1
    maximum: 100
  price:
    type: number
    exclusiveMinimum: 0
  rating:
    type: integer
    enum: [1, 2, 3, 4, 5]
```

### Strings

```yaml
properties:
  code:
    type: string
    minLength: 3
    maxLength: 10
  phone:
    type: string
    pattern: "^\\+[0-9]{10,15}$"
  status:
    type: string
    enum: ["pending", "approved", "rejected"]
```

`format` (`email`, `uri`, `uuid`, `date`, ...) is an annotation under the default JSON Schema
dialect and does not reject a value. It is checked when the tool schema declares
`$schema: "http://json-schema.org/draft-07/schema#"`; otherwise constrain the shape with `pattern`.

### Arrays

```yaml
properties:
  tags:
    type: array
    minItems: 1
    maxItems: 10
    items:
      type: string
      maxLength: 50
```

### Objects

```yaml
properties:
  address:
    type: object
    properties:
      street: { type: string }
      city: { type: string }
      zip: { type: string, pattern: "^[0-9]{5}$" }
    required: [street, city]
    additionalProperties: false   # no other fields allowed
```

---

## Violations

A call that breaks the policy fails the `args_valid` test: `assay run` exits 1 and names the test,
and `assay validate --trace-file` reports it as `E_ARGS_VIOLATION`. A policy that cannot be
enforced as written (a field this check does not read, a tool schema that constrains nothing) is a
configuration error before any test runs (exit 2). See
[args_valid](../metrics/args-valid.md#configuration-errors).

---

## Using Policies in Tests

Reference a policy file from a test in `eval.yaml`:

```yaml
# eval.yaml
configVersion: 1
suite: my-agent
model: trace

tests:
  - id: validate_all_args
    input:
      prompt: "Look up customer cust_42"
    expected:
      type: args_valid
      policy: policies/customer-service.yaml
```

### Inline Policies

For simple cases, put the schemas in the test:

```yaml
tests:
  - id: discount_limit
    input:
      prompt: "Apply a discount"
    expected:
      type: args_valid
      schema:
        apply_discount:
          type: object
          properties:
            percent: { type: number, maximum: 30 }
          required: [percent]
```

---

## Shared JSON Schema Definitions

Schema evaluation is hermetic: Assay does not retrieve file or HTTP references while compiling a
policy. Keep reusable definitions in the same document and reference them with a fragment:

```yaml
# policies/customer-service.yaml
version: "2.0"
schemas:
  $defs:
    customer_id:
      type: string
      pattern: "^cust_[0-9]+$"
    order_id:
      type: string
      pattern: "^ord_[0-9]+$"

  get_customer:
    type: object
    properties:
      id:
        $ref: "#/$defs/customer_id"

  get_order:
    type: object
    properties:
      order_id:
        $ref: "#/$defs/order_id"
```

Bundle definitions into the policy before execution when migrating a multi-file policy. External
`$ref` and `$dynamicRef` targets fail schema compilation; they are never fetched implicitly.

---

## Real-World Examples

### E-commerce Policy

```yaml
# policies/ecommerce.yaml
version: "2.0"
schemas:
  add_to_cart:
    type: object
    properties:
      product_id: { type: string }
      quantity: { type: integer, minimum: 1, maximum: 99 }
    required: [product_id, quantity]

  apply_coupon:
    type: object
    properties:
      code: { type: string, pattern: "^[A-Z0-9]{6,12}$" }
    required: [code]

  process_payment:
    type: object
    properties:
      amount: { type: number, minimum: 0.01, maximum: 10000 }
      currency: { type: string, enum: ["USD", "EUR", "GBP"] }
      card_token: { type: string }
    required: [amount, currency, card_token]
```

### Healthcare Policy

```yaml
# policies/healthcare.yaml
version: "2.0"
schemas:
  get_patient_record:
    type: object
    properties:
      patient_id: { type: string, pattern: "^P[0-9]{8}$" }
      include_history: { type: boolean }
    required: [patient_id]

  prescribe_medication:
    type: object
    properties:
      medication_id: { type: string }
      dosage_mg: { type: number, minimum: 0.1, maximum: 1000 }
      frequency: { type: string, enum: ["once_daily", "twice_daily", "as_needed"] }
    required: [medication_id, dosage_mg]
```

---

## Best Practices

### 1. Start Permissive, Then Tighten

Begin with types, then add bounds as you learn what the agent sends:

```yaml
# Week 1: just the type
percent:
  type: number

# Week 2: bounds after seeing outliers
percent:
  type: number
  minimum: 0
  maximum: 100

# Week 3: the business limit
percent:
  type: number
  minimum: 0
  maximum: 30
```

### 2. Use Descriptive Patterns

Document what patterns mean:

```yaml
properties:
  order_id:
    type: string
    pattern: "^ord_[0-9]{10}$"  # Format: ord_<10 digits>
```

### 3. Group Related Tools

Organize policies by domain:

```
policies/
├── customer.yaml      # Customer-related tools
├── payments.yaml      # Payment processing
├── notifications.yaml # Email, SMS, push
└── admin.yaml         # Administrative tools
```

### 4. Version Policies with Code

Policies should live in the same repo as your agent code:

```bash
git add policies/
git commit -m "Tighten discount limit to 30%"
```

---

## See Also

- [args_valid Metric](../metrics/args-valid.md)
- [Sequence Rules](../reference/config/sequences.md)
- [Migration Guide](../reference/config/migration.md)
