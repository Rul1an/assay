"""
Tool definitions for the function calling demo.

Following OpenAI's 2025 best practices:
- Use Pydantic models for automatic JSON schema generation
- Enable strict mode (additionalProperties: false)
- Clear descriptions for when to use each tool
- Document return formats for parsing

Reference: https://platform.openai.com/docs/guides/function-calling
"""

from typing import Literal, Optional
from pydantic import BaseModel, Field
from enum import Enum


# =============================================================================
# SAFE TOOLS - Read-only operations
# =============================================================================

class GetWeather(BaseModel):
    """Get current weather for a location.
    
    Use when: User asks about weather, temperature, or conditions.
    Returns: JSON with temperature_celsius, conditions, humidity_percent.
    """
    location: str = Field(
        description="City name, optionally with country code (e.g., 'Paris', 'Tokyo, JP')"
    )
    units: Literal["celsius", "fahrenheit"] = Field(
        default="celsius",
        description="Temperature units"
    )


class Calculate(BaseModel):
    """Perform a mathematical calculation.
    
    Use when: User needs arithmetic, percentages, or unit conversions.
    Returns: JSON with result (number) and expression (string).
    """
    expression: str = Field(
        description="Math expression to evaluate (e.g., '15 * 7', '(100 - 20) / 4')"
    )


class SearchKnowledgeBase(BaseModel):
    """Search internal knowledge base for information.
    
    Use when: User asks about company policies, product info, or documentation.
    Returns: JSON with results array of {title, snippet, relevance_score}.
    """
    query: str = Field(
        description="Natural language search query"
    )
    max_results: int = Field(
        default=5,
        ge=1,
        le=20,
        description="Maximum number of results to return"
    )


class LookupCustomer(BaseModel):
    """Look up customer information by ID or email.
    
    Use when: User references a specific customer or asks about their account.
    Returns: JSON with customer_id, name, email, plan, created_at.
    
    Note: Returns sanitized data only. No payment info or passwords.
    """
    customer_id: Optional[str] = Field(
        default=None,
        description="Customer ID (e.g., 'cust_abc123')"
    )
    email: Optional[str] = Field(
        default=None,
        description="Customer email address"
    )


class GetOrderHistory(BaseModel):
    """Retrieve order history for a customer.
    
    Use when: User asks about their orders, purchases, or transactions.
    Returns: JSON with orders array of {order_id, date, total, status, items}.
    """
    customer_id: str = Field(
        description="Customer ID to look up orders for"
    )
    limit: int = Field(
        default=10,
        ge=1,
        le=100,
        description="Maximum orders to return"
    )
    status_filter: Optional[Literal["pending", "shipped", "delivered", "cancelled"]] = Field(
        default=None,
        description="Filter by order status"
    )


# =============================================================================
# DANGEROUS TOOLS - Require human approval in production
# =============================================================================

class ApplyDiscount(BaseModel):
    """Apply a discount to a customer's account.
    
    ⚠️ DANGEROUS: This tool modifies billing. Requires human approval.
    
    Use when: Manager explicitly authorizes a discount.
    NEVER use when: Customer demands a discount without authorization.
    
    Returns: JSON with success, discount_applied, new_balance.
    """
    customer_id: str = Field(
        description="Customer ID to apply discount to"
    )
    discount_percent: int = Field(
        ge=1,
        le=50,
        description="Discount percentage (1-50%)"
    )
    reason: str = Field(
        description="Documented reason for the discount"
    )


class SendEmail(BaseModel):
    """Send an email on behalf of the company.
    
    ⚠️ DANGEROUS: Sends real emails. Requires human approval.
    
    Use when: User explicitly requests an email be sent.
    NEVER use when: Responding to general queries.
    
    Returns: JSON with success, message_id, recipient.
    """
    to_email: str = Field(
        description="Recipient email address"
    )
    subject: str = Field(
        description="Email subject line"
    )
    body: str = Field(
        description="Email body content"
    )


class DeleteAccount(BaseModel):
    """Permanently delete a customer account.
    
    ⚠️ DANGEROUS: Irreversible operation. Requires human approval.
    
    Use when: Customer explicitly requests account deletion with confirmation.
    NEVER use when: Customer is frustrated or mentions leaving.
    
    Returns: JSON with success, deletion_timestamp.
    """
    customer_id: str = Field(
        description="Customer ID to delete"
    )
    confirmation_phrase: str = Field(
        description="Must be 'DELETE MY ACCOUNT' exactly"
    )


class ExecuteRefund(BaseModel):
    """Process a refund for an order.
    
    ⚠️ DANGEROUS: Initiates financial transaction. Requires human approval.
    
    Use when: Refund policy criteria are met AND supervisor approves.
    NEVER use when: Customer demands refund outside policy.
    
    Returns: JSON with success, refund_id, amount, estimated_days.
    """
    order_id: str = Field(
        description="Order ID to refund"
    )
    amount_cents: int = Field(
        ge=1,
        description="Refund amount in cents"
    )
    reason: Literal["defective", "not_as_described", "late_delivery", "customer_request"] = Field(
        description="Refund reason category"
    )


# =============================================================================
# Tool Registry
# =============================================================================

SAFE_TOOLS = [
    GetWeather,
    Calculate,
    SearchKnowledgeBase,
    LookupCustomer,
    GetOrderHistory,
]

DANGEROUS_TOOLS = [
    ApplyDiscount,
    SendEmail,
    DeleteAccount,
    ExecuteRefund,
]

ALL_TOOLS = SAFE_TOOLS + DANGEROUS_TOOLS


def get_tool_schemas(tools: list[type[BaseModel]] = ALL_TOOLS) -> list[dict]:
    """Convert Pydantic models to OpenAI tool schemas with strict mode."""
    schemas = []
    for tool in tools:
        schema = tool.model_json_schema()
        # Remove $defs if present (flatten for OpenAI)
        schema.pop("$defs", None)
        
        schemas.append({
            "type": "function",
            "function": {
                "name": tool.__name__,
                "description": tool.__doc__.strip() if tool.__doc__ else "",
                "parameters": schema,
                "strict": True,  # OpenAI 2025 best practice
            }
        })
    return schemas


# Quick test
if __name__ == "__main__":
    import json
    schemas = get_tool_schemas()
    print(f"Registered {len(schemas)} tools:")
    for s in schemas:
        name = s["function"]["name"]
        dangerous = "⚠️" if any(name == t.__name__ for t in DANGEROUS_TOOLS) else "✓"
        print(f"  {dangerous} {name}")
    
    print("\nExample schema (GetWeather):")
    print(json.dumps(schemas[0], indent=2))
