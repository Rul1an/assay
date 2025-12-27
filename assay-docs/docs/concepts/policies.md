# Policies

Policies define what "correct" means for your AI agent's tool usage.

---

## What is a Policy?

A **policy** is a set of rules that validate tool arguments:

- Data types (string, number, boolean)
- Value constraints (min, max, pattern)
- Required fields
- Custom validation logic

When Assay replays a trace, it checks every tool call against your policies. If an argument violates a rule, the test fails.

---

## Policy Structure

Policies are YAML files organized by tool:

```yaml
# policies/customer-service.yaml
tools:
  get_customer:
    arguments:
      id:
        type: string
        required: true
        pattern: "^cust_[0-9]+$"
  
  apply_discount:
    arguments:
      percent:
        type: number
        min: 0
        max: 30
      order_id:
        type: string
        required: true
  
  send_email:
    arguments:
      to:
        type: string
        format: email
      subject:
        type: string
        maxLength: 200
```

---

## Constraint Types

### Type Validation

```yaml
arguments:
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

### Required Fields

```yaml
arguments:
  id:
    required: true  # Must be present
  nickname:
    required: false  # Optional (default)
```

### Number Constraints

```yaml
arguments:
  quantity:
    type: number
    min: 1
    max: 100
    
  price:
    type: number
    minimum: 0        # Alias for min
    maximum: 9999.99  # Alias for max
    
  rating:
    type: number
    enum: [1, 2, 3, 4, 5]  # Must be one of these values
```

### String Constraints

```yaml
arguments:
  code:
    type: string
    minLength: 3
    maxLength: 10
    
  email:
    type: string
    format: email  # Built-in format
    
  phone:
    type: string
    pattern: "^\\+[0-9]{10,15}$"  # Regex pattern
    
  status:
    type: string
    enum: ["pending", "approved", "rejected"]
```

### Built-in Formats

| Format | Validates |
|--------|-----------|
| `email` | Valid email address |
| `uri` | Valid URI/URL |
| `uuid` | UUID v4 format |
| `date` | ISO 8601 date (YYYY-MM-DD) |
| `datetime` | ISO 8601 datetime |
| `ipv4` | IPv4 address |
| `ipv6` | IPv6 address |

```yaml
arguments:
  user_email:
    type: string
    format: email
  
  webhook_url:
    type: string
    format: uri
  
  request_id:
    type: string
    format: uuid
```

### Array Constraints

```yaml
arguments:
  tags:
    type: array
    minItems: 1
    maxItems: 10
    items:
      type: string
      maxLength: 50
  
  scores:
    type: array
    items:
      type: number
      min: 0
      max: 100
```

### Object Constraints

```yaml
arguments:
  address:
    type: object
    properties:
      street:
        type: string
        required: true
      city:
        type: string
        required: true
      zip:
        type: string
        pattern: "^[0-9]{5}$"
    additionalProperties: false  # No extra fields allowed
```

---

## Violation Actions

Control what happens when a rule is violated:

```yaml
arguments:
  percent:
    type: number
    max: 30
    on_violation: block   # Default: fail the test
  
  legacy_field:
    type: string
    on_violation: warn    # Log warning, don't fail
  
  debug_mode:
    type: boolean
    on_violation: log     # Silent log, continue
```

| Action | Behavior |
|--------|----------|
| `block` | Fail the test (default) |
| `warn` | Log warning, test continues |
| `log` | Silent log, test continues |

---

## Using Policies in Tests

Reference policies in your `mcp-eval.yaml`:

```yaml
# mcp-eval.yaml
version: "1"
suite: my-agent

tests:
  - id: validate_all_args
    metric: args_valid
    policy: policies/customer-service.yaml
  
  - id: validate_payments_only
    metric: args_valid
    policy: policies/payments.yaml
    tools: [process_payment, refund]  # Only check these tools
```

### Inline Policies

For simple cases, define policies inline:

```yaml
tests:
  - id: discount_limit
    metric: args_valid
    tool: apply_discount
    constraints:
      percent:
        type: number
        max: 30
```

---

## Policy Inheritance

Use `$ref` to share common definitions:

```yaml
# policies/common.yaml
definitions:
  customer_id:
    type: string
    pattern: "^cust_[0-9]+$"
  
  order_id:
    type: string
    pattern: "^ord_[0-9]+$"

# policies/customer-service.yaml
tools:
  get_customer:
    arguments:
      id:
        $ref: "common.yaml#/definitions/customer_id"
  
  get_order:
    arguments:
      order_id:
        $ref: "common.yaml#/definitions/order_id"
```

---

## Real-World Examples

### E-commerce Policy

```yaml
# policies/ecommerce.yaml
tools:
  add_to_cart:
    arguments:
      product_id:
        type: string
        required: true
      quantity:
        type: number
        min: 1
        max: 99
  
  apply_coupon:
    arguments:
      code:
        type: string
        pattern: "^[A-Z0-9]{6,12}$"
      
  process_payment:
    arguments:
      amount:
        type: number
        min: 0.01
        max: 10000
      currency:
        type: string
        enum: ["USD", "EUR", "GBP"]
      card_token:
        type: string
        required: true
```

### Healthcare Policy

```yaml
# policies/healthcare.yaml
tools:
  get_patient_record:
    arguments:
      patient_id:
        type: string
        required: true
        pattern: "^P[0-9]{8}$"
      include_history:
        type: boolean
  
  prescribe_medication:
    arguments:
      medication_id:
        type: string
        required: true
      dosage_mg:
        type: number
        min: 0.1
        max: 1000
      frequency:
        type: string
        enum: ["once_daily", "twice_daily", "as_needed"]
```

---

## Error Messages

When validation fails, Assay provides actionable feedback:

```
❌ FAIL: args_valid

   Tool: apply_discount
   Argument: percent = 50
   Violation: Value exceeds maximum (max: 30)
   Policy: policies/customer-service.yaml:12

   Suggestion: Use percent <= 30

   Docs: https://docs.assay.dev/config/policies
```

---

## Best Practices

### 1. Start Permissive, Then Tighten

Begin with type validation only, then add constraints as you discover edge cases:

```yaml
# Week 1: Just types
arguments:
  percent:
    type: number

# Week 2: Add bounds after seeing outliers
arguments:
  percent:
    type: number
    min: 0
    max: 100

# Week 3: Tighten based on business rules
arguments:
  percent:
    type: number
    min: 0
    max: 30  # Business limit
```

### 2. Use Descriptive Patterns

Document what patterns mean:

```yaml
arguments:
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
- [Sequence Rules](../config/sequences.md)
- [Migration Guide](../config/migration.md)
