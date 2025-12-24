def GetWeather(args):
    loc = args.get("location", "Unknown")
    return {"location": loc, "temperature_celsius": 22, "conditions": "partly cloudy"}


def DeleteAccount(args):
    raise ValueError("Action Forbidden")
