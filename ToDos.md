# ToDos:
## General:

- [X] Configuration File: Instead of hardcoding the routes (like google.localhost), you could use a configuration file (e.g., in TOML or YAML format) to define the mappings from incoming hosts/paths to upstream servers. This would make your proxy much more flexible.
- [X] Health Checks: You could add a mechanism to periodically check the health of your upstream servers and automatically remove unhealthy servers from the routing pool.
- [X] Improved Error Handling: Implement more robust error handling. For example, if an upstream server is unavailable, you could return a 502 Bad Gateway error to the client.
- [X] Load Balancing: For a single route, you could configure multiple upstream servers and implement a load balancing strategy (like round-robin) to distribute the load between them.
- [X] A catch all/default route to handle requests that don't match any of the configured routes.
- [X] Handle satic file serving directly from the filesystem.
- [X] Middleware support to add middlewares to the proxy.

## Middlewares:

- [X] Logging and Tracing: Add more detailed logging to get better insight into the requests and responses being proxied, via middleware.
- [X] Basic Authentication: Implement basic authentication to secure the proxy, via middleware.
- [X] Rate Limiting: Implement rate limiting to prevent abuse, via middleware.
- [ ] Compression: Implement compression to reduce the size of the proxied responses, via middleware.
- [ ] Caching: Implement caching to improve performance, via middleware.

## Advanced:
- [ ] Dynamic Routing: Based on the configuration file, you could dynamically add, remove, or modify routes without restarting the server.
- [ ] Dashboard: Provide a dashboard to monitor the health of the proxy and its upstream servers.

## HTTPS Support:

- [ ] Client-facing: Add TLS termination to handle HTTPS requests from clients.
- [ ] Upstream: Allow proxying to upstream servers that use HTTPS.
