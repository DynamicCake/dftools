# xPlot
xPlot (Cross plot) is a general purpose cross plot messaging service.

# TODO Implement
## `/restrict`
- GET - Returns
```jsonc
{
    "type": "allow",
    "plots": [41808]
}
```
- POST (type: String, plots: List(Int)) - Completely replace the restrictions

## `/message`
- GET (since: Int, keep: Bool = false)
    - Returns the messages fifo order
    - If keep is false, the messages get deleted
    - Messages get deleted when the instance decides to
```jsonc
{
    "plot_origin": 41808, // The plot that sent this message
    "id": 243,
    "timestamp": 1743544800 // Time sent
    "data": { // Payload (DFJSON)
        "id": "str",
        "val": "Hello world!"
    }
}
```
- POST (data: DfValue) - add some data to the message queue
```jsonc
{
    "destination": 41809, // Plot to send to
    "data": { // Payload (DFJSON)
        "id": "str",
        "val": "Hello world!"
    }
}
```
