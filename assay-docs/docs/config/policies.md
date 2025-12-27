# Policy Files

Detailed reference for policy YAML configuration.

---

## Overview

Policy files define validation rules for tool arguments. They're YAML files that specify:

- Argument types (string, number, boolean, etc.)
- Constraints (min, max, pattern, enum, etc.)
- Required fields
- Violation actions

---

## File Structure

```yaml
# policies/customer-service.yaml

# Optional metadata
description: "Customer service agent policies"
version: "1.0"

# Tool definitions
tools:
  tool_name:
    description: "Optional description"
    arguments:
      arg_name:
        type: string
        # ... constraints
```

---

## Tool Definitions

### Basic Structure

```yaml
tools:
  get_customer:
    arguments:
      id:
        type: string
        required: true
```

### With Description

```yaml
tools:
  apply_discount:
    description: "Apply a percentage discount to an order"
    arguments:
      percent:
        type: number
        description: "Discount percentage (0-30)"
        min: 0
        max: 30
```

---

## Type Validation

### Primitive Types

```yaml
arguments:
  # String
  name:
    type: string
  
  # Number (integer or float)
  amount:
    type: number
  
  # Integer only
  count:
    type: integer
  
  # Boolean
  active:
    type: boolean
```

### Complex Types

```yaml
arguments:
  # Array
  tags:
    type: array
    items:
      type: string
  
  # Object
  address:
    type: object
    properties:
      street: { type: string }
      city: { type: string }
```

---

## Constraints

### String Constraints

```yaml
arguments:
  code:
    type: string
    minLength: 3          # Minimum length
    maxLength: 10         # Maximum length
    pattern: "^[A-Z]+$"   # Regex pattern
    format: email         # Built-in format
    enum:                 # Allowed values
      - "pending"
      - "approved"
      - "rejected"
```

### Number Constraints

```yaml
arguments:
  price:
    type: number
    min: 0                # Minimum value (inclusive)
    max: 9999.99          # Maximum value (inclusive)
    exclusiveMin: 0       # Minimum (exclusive)
    exclusiveMax: 10000   # Maximum (exclusive)
    multipleOf: 0.01      # Must be multiple of
    enum: [1, 2, 3, 4, 5] # Allowed values
```

### Array Constraints

```yaml
arguments:
  items:
    type: array
    minItems: 1           # Minimum items
    maxItems: 100         # Maximum items
    uniqueItems: true     # No duplicates
    items:                # Item schema
      type: string
      maxLength: 50
```

### Object Constraints

```yaml
arguments:
  config:
    type: object
    properties:
      enabled: { type: boolean }
      threshold: { type: number, min: 0 }
    required:
      - enabled
    additionalProperties: false  # No extra fields
```

---

## Built-in Formats

| Format | Validates | Example |
|--------|-----------|---------|
| `email` | Email address | `user@example.com` |
| `uri` | URI/URL | `https://example.com` |
| `uuid` | UUID v4 | `550e8400-e29b-41d4-a716-446655440000` |
| `date` | ISO date | `2025-12-27` |
| `datetime` | ISO datetime | `2025-12-27T10:00:00Z` |
| `time` | ISO time | `10:00:00` |
| `ipv4` | IPv4 address | `192.168.1.1` |
| `ipv6` | IPv6 address | `::1` |
| `hostname` | Hostname | `example.com` |

```yaml
arguments:
  email:
    type: string
    format: email
  
  created_at:
    type: string
    format: datetime
```

---

## Required Fields

```yaml
arguments:
  id:
    type: string
    required: true     # Must be present
  
  nickname:
    type: string
    required: false    # Optional (default)
```

---

## Violation Actions

Control behavior when validation fails:

```yaml
arguments:
  percent:
    type: number
    max: 30
    on_violation: block   # Fail the test (default)
  
  legacy_field:
    type: string
    on_violation: warn    # Log warning, continue
  
  debug_mode:
    type: boolean
    on_violation: log     # Silent log, continue
```

| Action | Test Result | Logs |
|--------|-------------|------|
| `block` | ❌ Fail | Error logged |
| `warn` | ✅ Pass | Warning logged |
| `log` | ✅ Pass | Debug logged |

---

## References ($ref)

Share definitions across tools:

```yaml
# policies/common.yaml
definitions:
  customer_id:
    type: string
    pattern: "^cust_[0-9]+$"
    description: "Customer ID format: cust_<digits>"

# policies/customer.yaml
tools:
  get_customer:
    arguments:
      id:
        $ref: "common.yaml#/definitions/customer_id"
  
  update_customer:
    arguments:
      id:
        $ref: "common.yaml#/definitions/customer_id"
      email:
        type: string
        format: email
```

---

## Conditional Validation

*(Advanced, v1.1+)*

```yaml
arguments:
  payment_type:
    type: string
    enum: ["card", "bank_transfer"]
  
  card_number:
    type: string
    pattern: "^[0-9]{16}$"
    required_if:
      payment_type: "card"
  
  account_number:
    type: string
    required_if:
      payment_type: "bank_transfer"
```

---

## Complete Example

```yaml
# policies/ecommerce.yaml
description: "E-commerce agent validation rules"
version: "1.0"

tools:
  add_to_cart:
    description: "Add item to shopping cart"
    arguments:
      product_id:
        type: string
        required: true
        pattern: "^prod_[a-z0-9]+$"
      quantity:
        type: integer
        required: true
        min: 1
        max: 99

  apply_coupon:
    description: "Apply a coupon code"
    arguments:
      code:
        type: string
        required: true
        pattern: "^[A-Z0-9]{6,12}$"
        description: "Coupon code (6-12 alphanumeric chars)"

  process_payment:
    description: "Process payment for order"
    arguments:
      order_id:
        type: string
        required: true
      amount:
        type: number
        required: true
        min: 0.01
        max: 10000
      currency:
        type: string
        required: true
        enum: ["USD", "EUR", "GBP"]
      card_token:
        type: string
        required: true
        description: "Tokenized card (never raw card numbers)"

  refund:
    description: "Process refund"
    arguments:
      order_id:
        type: string
        required: true
      amount:
        type: number
        required: true
        min: 0.01
      reason:
        type: string
        required: true
        enum:
          - "customer_request"
          - "item_defective"
          - "wrong_item"
          - "other"
```

---

## See Also

- [Policies Concept](../concepts/policies.md)
- [args_valid Metric](../metrics/args-valid.md)
- [Sequence Rules](sequences.md)
