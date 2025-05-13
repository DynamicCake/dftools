# Baton
Baton streamlines sending data when doing a plot transfer, passing the baton.

**WARNING**: Do **not** use this for "referrals" as that, is a bad idea.
A rouge trusted plot could constantly keep setting the `transfer` value taking all the credit.

TODO: Link to OpenAPI spec

# TODO Implement
## `/trusted`
Plots need to be trusted because a rouge trusted plot could constantly keep setting the `transfer` value taking all the credit.

GET - Returns all trusted plots -> List(Int)
POST - Replaces the trusted plot list
## `/transfer`
- GET (uuid: String) - Returns
```jsonc
{
    "plot_origin": 41808, // The plot id that sent the transfer
    "time_set": 1743544800, // The time the plot claimed to send the transfer
    "data": { // Payload (DFJSON)
        "id": "str",
        "val": "Hello world!"
    }
}
```
- POST (plot_id: Int, data: DfValue) - Add some data before sending user
- DELETE (uuid: String) - Deletes and returns `GET`, you should be using this instead


## `/message/poll`
- GET - returns the newest version number


