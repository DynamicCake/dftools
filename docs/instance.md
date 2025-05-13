# Instance
Instance is the API to register, edit, and view plots from this or other instances.

# DFTools Instance Cooperation
It was decided that allowing a since centralized server instance to dominate DiamondFire is bad.

Because of this, DFTools is designed with other instances in mind.
For example: To allow xPlot to send messages to other plots from other instances you would:
1. Register your plot on the other instance with `POST /instance/v1/plot` and provide your instance domain
2. Get allow listed on the target plot
3. Send a message!

TODO: Link to OpenAPI spec

