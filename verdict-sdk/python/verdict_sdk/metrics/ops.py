def passes_threshold(value: float, threshold: float, op: str) -> bool:
    if op == ">=":
        return value >= threshold
    if op == ">":
        return value > threshold
    if op == "<=":
        return value <= threshold
    if op == "<":
        return value < threshold
    # Default fallback or error?
    raise ValueError(f"Unknown compare op: {op}")
