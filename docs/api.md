# AdapterOS Server API Documentation

This document contains the complete OpenAPI specification for the AdapterOS Server API.

## Overview

The AdapterOS Server API provides endpoints for managing tenants, adapters, repositories, training jobs, and more in the AdapterOS system.

## Demo Credentials

The following demo credentials are available for testing:

<<<<<<< HEAD
- **Admin:** admin@example.com / password
=======
- **Admin:** admin@aos.local / password
- **Operator:** operator@aos.local / password  
- **SRE:** sre@aos.local / password
- **Viewer:** viewer@aos.local / password
>>>>>>> integration-branch

## OpenAPI Specification

```json
{
  "openapi": "3.0.3",
  "info": {
<<<<<<< HEAD
    "title": "AdapterOS Server API",
    "description": "Complete API for AdapterOS system management",
    "version": "1.0.0",
    "license": {
      "name": "Apache-2.0"
    }
  },
  "servers": [
    {
      "url": "http://localhost:8080/api",
      "description": "Development server"
    }
  ],
  "security": [
    {
      "bearer_token": []
    }
  ],
  "paths": {
    "/healthz": {
      "get": {
        "tags": ["health"],
        "summary": "Health check endpoint",
        "operationId": "health_check",
=======
    "title": "adapteros-server-api",
    "description": "",
    "license": {
      "name": ""
    },
    "version": "0.1.0"
  },
  "paths": {
    "/api/v1/patch/propose": {
      "post": {
        "tags": [
          "handlers"
        ],
        "summary": "Propose a patch for code changes",
        "operationId": "propose_patch",
        "requestBody": {
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/ProposePatchRequest"
              }
            }
          },
          "required": true
        },
        "responses": {
          "200": {
            "description": "Patch proposal created",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ProposePatchResponse"
                }
              }
            }
          },
          "400": {
            "description": "Invalid request"
          },
          "401": {
            "description": "Unauthorized"
          },
          "500": {
            "description": "Internal server error"
          }
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
      }
    },
    "/healthz": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "Health check endpoint",
        "operationId": "health",
>>>>>>> integration-branch
        "responses": {
          "200": {
            "description": "Service is healthy",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/HealthResponse"
                }
              }
            }
          }
        }
      }
    },
    "/readyz": {
      "get": {
<<<<<<< HEAD
        "tags": ["health"],
        "summary": "Readiness check",
        "operationId": "readiness_check",
=======
        "tags": [
          "handlers"
        ],
        "summary": "Readiness check",
        "operationId": "ready",
>>>>>>> integration-branch
        "responses": {
          "200": {
            "description": "Service is ready",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/HealthResponse"
                }
              }
            }
<<<<<<< HEAD
=======
          },
          "503": {
            "description": "Service is not ready",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/HealthResponse"
                }
              }
            }
>>>>>>> integration-branch
          }
        }
      }
    },
<<<<<<< HEAD
    "/metrics": {
      "get": {
        "tags": ["metrics"],
        "summary": "Get Prometheus metrics",
        "operationId": "get_metrics",
        "responses": {
          "200": {
            "description": "Metrics data",
            "content": {
              "text/plain": {
                "schema": {
                  "type": "string"
=======
    "/v1/adapters": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "List all adapters",
        "operationId": "list_adapters",
        "parameters": [
          {
            "name": "tier",
            "in": "query",
            "description": "Filter by tier",
            "required": false,
            "schema": {
              "type": "integer",
              "format": "int32",
              "nullable": true
            }
          },
          {
            "name": "framework",
            "in": "query",
            "description": "Filter by framework",
            "required": false,
            "schema": {
              "type": "string",
              "nullable": true
            }
          }
        ],
        "responses": {
          "200": {
            "description": "List of adapters",
            "content": {
              "application/json": {
                "schema": {
                  "type": "array",
                  "items": {
                    "$ref": "#/components/schemas/AdapterResponse"
                  }
                }
              }
            }
          },
          "401": {
            "description": "Unauthorized",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/adapters/register": {
      "post": {
        "tags": [
          "handlers"
        ],
        "summary": "Register new adapter",
        "operationId": "register_adapter",
        "requestBody": {
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/RegisterAdapterRequest"
              }
            }
          },
          "required": true
        },
        "responses": {
          "201": {
            "description": "Adapter registered",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/AdapterResponse"
                }
              }
            }
          },
          "400": {
            "description": "Invalid request",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/adapters/{adapter_id}": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "Get adapter by ID",
        "operationId": "get_adapter",
        "parameters": [
          {
            "name": "adapter_id",
            "in": "path",
            "description": "Adapter ID",
            "required": true,
            "schema": {
              "type": "string"
            }
          }
        ],
        "responses": {
          "200": {
            "description": "Adapter details",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/AdapterResponse"
                }
              }
            }
          },
          "404": {
            "description": "Adapter not found",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          }
        }
      },
      "delete": {
        "tags": [
          "handlers"
        ],
        "summary": "Delete adapter",
        "operationId": "delete_adapter",
        "parameters": [
          {
            "name": "adapter_id",
            "in": "path",
            "description": "Adapter ID",
            "required": true,
            "schema": {
              "type": "string"
            }
          }
        ],
        "responses": {
          "204": {
            "description": "Adapter deleted"
          },
          "404": {
            "description": "Adapter not found",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/adapters/{adapter_id}/activations": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "Get adapter activations",
        "operationId": "get_adapter_activations",
        "parameters": [
          {
            "name": "adapter_id",
            "in": "path",
            "description": "Adapter ID",
            "required": true,
            "schema": {
              "type": "string"
            }
          },
          {
            "name": "limit",
            "in": "query",
            "description": "Limit results (default: 100)",
            "required": false,
            "schema": {
              "type": "integer",
              "format": "int64",
              "nullable": true
            }
          }
        ],
        "responses": {
          "200": {
            "description": "Activation history",
            "content": {
              "application/json": {
                "schema": {
                  "type": "array",
                  "items": {
                    "$ref": "#/components/schemas/AdapterActivationResponse"
                  }
>>>>>>> integration-branch
                }
              }
            }
          }
        }
      }
    },
    "/v1/auth/login": {
      "post": {
<<<<<<< HEAD
        "tags": ["authentication"],
        "summary": "Login with credentials",
        "operationId": "auth_login",
        "requestBody": {
          "required": true,
=======
        "tags": [
          "handlers"
        ],
        "summary": "Login handler",
        "operationId": "auth_login",
        "requestBody": {
>>>>>>> integration-branch
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/LoginRequest"
              }
            }
<<<<<<< HEAD
          }
=======
          },
          "required": true
>>>>>>> integration-branch
        },
        "responses": {
          "200": {
            "description": "Login successful",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/LoginResponse"
                }
              }
            }
          },
          "401": {
            "description": "Invalid credentials",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          }
        }
      }
    },
<<<<<<< HEAD
    "/v1/auth/logout": {
      "post": {
        "tags": ["authentication"],
        "summary": "Logout current session",
        "operationId": "auth_logout",
        "responses": {
          "200": {
            "description": "Logout successful"
          }
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
      }
    },
    "/v1/auth/me": {
      "get": {
        "tags": ["authentication"],
        "summary": "Get current user info",
        "operationId": "auth_me",
        "responses": {
          "200": {
            "description": "User information",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/UserInfo"
                }
              }
            }
          }
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
      }
    },
    "/v1/models": {
      "get": {
        "tags": ["models"],
        "summary": "List available models",
        "operationId": "list_models",
        "responses": {
          "200": {
            "description": "List of models",
=======
    "/v1/commits": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "List commits",
        "operationId": "list_commits",
        "parameters": [
          {
            "name": "repo_id",
            "in": "query",
            "description": "Filter by repository",
            "required": false,
            "schema": {
              "type": "string",
              "nullable": true
            }
          },
          {
            "name": "branch",
            "in": "query",
            "description": "Filter by branch",
            "required": false,
            "schema": {
              "type": "string",
              "nullable": true
            }
          },
          {
            "name": "limit",
            "in": "query",
            "description": "Limit results",
            "required": false,
            "schema": {
              "type": "integer",
              "format": "int64",
              "nullable": true
            }
          }
        ],
        "responses": {
          "200": {
            "description": "List of commits",
>>>>>>> integration-branch
            "content": {
              "application/json": {
                "schema": {
                  "type": "array",
                  "items": {
<<<<<<< HEAD
                    "$ref": "#/components/schemas/ModelInfo"
=======
                    "$ref": "#/components/schemas/CommitResponse"
>>>>>>> integration-branch
                  }
                }
              }
            }
          }
<<<<<<< HEAD
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
      }
    },
    "/v1/models/import": {
      "post": {
        "tags": ["models"],
        "summary": "Import a new model",
        "operationId": "import_model",
        "requestBody": {
          "required": true,
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/ImportModelRequest"
              }
            }
          }
        },
        "responses": {
          "200": {
            "description": "Model import started",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ImportModelResponse"
                }
              }
            }
          }
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
      }
    },
    "/v1/models/{model_id}": {
      "get": {
        "tags": ["models"],
        "summary": "Get model details",
        "operationId": "get_model",
        "parameters": [
          {
            "name": "model_id",
            "in": "path",
            "required": true,
            "schema": {
              "type": "string"
            },
            "description": "Model ID"
=======
        }
      }
    },
    "/v1/commits/{sha}": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "Get commit details",
        "operationId": "get_commit",
        "parameters": [
          {
            "name": "sha",
            "in": "path",
            "description": "Commit SHA",
            "required": true,
            "schema": {
              "type": "string"
            }
>>>>>>> integration-branch
          }
        ],
        "responses": {
          "200": {
<<<<<<< HEAD
            "description": "Model details",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ModelDetails"
=======
            "description": "Commit details",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/CommitResponse"
>>>>>>> integration-branch
                }
              }
            }
          },
          "404": {
<<<<<<< HEAD
            "description": "Model not found",
=======
            "description": "Commit not found",
>>>>>>> integration-branch
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          }
<<<<<<< HEAD
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
      },
      "delete": {
        "tags": ["models"],
        "summary": "Delete a model",
        "operationId": "delete_model",
        "parameters": [
          {
            "name": "model_id",
            "in": "path",
            "required": true,
            "schema": {
              "type": "string"
            },
            "description": "Model ID"
          }
        ],
        "responses": {
          "204": {
            "description": "Model deleted"
          },
          "404": {
            "description": "Model not found"
          }
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
      }
    },
    "/v1/adapters": {
      "get": {
        "tags": ["adapters"],
        "summary": "List all adapters",
        "operationId": "list_adapters",
        "responses": {
          "200": {
            "description": "List of adapters",
=======
        }
      }
    },
    "/v1/commits/{sha}/diff": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "Get commit diff",
        "operationId": "get_commit_diff",
        "parameters": [
          {
            "name": "sha",
            "in": "path",
            "description": "Commit SHA",
            "required": true,
            "schema": {
              "type": "string"
            }
          }
        ],
        "responses": {
          "200": {
            "description": "Commit diff",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/CommitDiffResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/contacts": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "List contacts with filtering",
        "description": "Returns contacts discovered during inference, filtered by tenant and optionally by category.\nContacts represent entities (users, adapters, repositories, systems) that the inference\nengine has interacted with.\n\nCitation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6",
        "operationId": "list_contacts",
        "parameters": [
          {
            "name": "tenant",
            "in": "query",
            "description": "Tenant ID",
            "required": true,
            "schema": {
              "type": "string"
            }
          },
          {
            "name": "category",
            "in": "query",
            "description": "Filter by category (user|system|adapter|repository|external)",
            "required": false,
            "schema": {
              "type": "string",
              "nullable": true
            }
          },
          {
            "name": "limit",
            "in": "query",
            "description": "Limit results (default: 100)",
            "required": false,
            "schema": {
              "type": "integer",
              "nullable": true,
              "minimum": 0
            }
          }
        ],
        "responses": {
          "200": {
            "description": "List of contacts",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ContactsResponse"
                }
              }
            }
          },
          "400": {
            "description": "Invalid request",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          },
          "500": {
            "description": "Server error",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          }
        }
      },
      "post": {
        "tags": [
          "handlers"
        ],
        "summary": "Create or update a contact",
        "description": "Creates a new contact or updates an existing one based on (tenant_id, name, category) uniqueness.\nThis endpoint can be used to manually register contacts or update their metadata.\n\nCitation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6",
        "operationId": "create_contact",
        "requestBody": {
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/CreateContactRequest"
              }
            }
          },
          "required": true
        },
        "responses": {
          "200": {
            "description": "Contact created/updated",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ContactResponse"
                }
              }
            }
          },
          "400": {
            "description": "Invalid request",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          },
          "500": {
            "description": "Server error",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/contacts/{id}": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "Get contact by ID",
        "description": "Retrieves a specific contact by its unique identifier.\n\nCitation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6",
        "operationId": "get_contact",
        "parameters": [
          {
            "name": "id",
            "in": "path",
            "description": "Contact ID",
            "required": true,
            "schema": {
              "type": "string"
            }
          }
        ],
        "responses": {
          "200": {
            "description": "Contact details",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ContactResponse"
                }
              }
            }
          },
          "404": {
            "description": "Contact not found",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          },
          "500": {
            "description": "Server error",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          }
        }
      },
      "delete": {
        "tags": [
          "handlers"
        ],
        "summary": "Delete a contact",
        "description": "Permanently deletes a contact and all associated interaction records.\n\nCitation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6",
        "operationId": "delete_contact",
        "parameters": [
          {
            "name": "id",
            "in": "path",
            "description": "Contact ID",
            "required": true,
            "schema": {
              "type": "string"
            }
          }
        ],
        "responses": {
          "200": {
            "description": "Contact deleted"
          },
          "404": {
            "description": "Contact not found",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          },
          "500": {
            "description": "Server error",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/contacts/{id}/interactions": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "Get contact interaction history",
        "description": "Returns the interaction log for a specific contact, showing when and how\nthe contact was referenced during inference operations.\n\nCitation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6",
        "operationId": "get_contact_interactions",
        "parameters": [
          {
            "name": "id",
            "in": "path",
            "description": "Contact ID",
            "required": true,
            "schema": {
              "type": "string"
            }
          },
          {
            "name": "limit",
            "in": "query",
            "description": "Limit results (default: 50)",
            "required": false,
            "schema": {
              "type": "integer",
              "nullable": true,
              "minimum": 0
            }
          }
        ],
        "responses": {
          "200": {
            "description": "Interaction history",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ContactInteractionsResponse"
                }
              }
            }
          },
          "404": {
            "description": "Contact not found",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          },
          "500": {
            "description": "Server error",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/domain-adapters": {
      "get": {
        "tags": [
          "domain_adapters"
        ],
        "summary": "List all domain adapters",
        "operationId": "list_domain_adapters",
        "responses": {
          "200": {
            "description": "List of domain adapters",
>>>>>>> integration-branch
            "content": {
              "application/json": {
                "schema": {
                  "type": "array",
                  "items": {
<<<<<<< HEAD
                    "$ref": "#/components/schemas/AdapterInfo"
=======
                    "$ref": "#/components/schemas/DomainAdapterResponse"
>>>>>>> integration-branch
                  }
                }
              }
            }
<<<<<<< HEAD
          }
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
      },
      "post": {
        "tags": ["adapters"],
        "summary": "Register new adapter",
        "operationId": "register_adapter",
        "requestBody": {
          "required": true,
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/RegisterAdapterRequest"
              }
            }
          }
        },
        "responses": {
          "201": {
            "description": "Adapter registered",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/AdapterRegistrationResponse"
=======
          },
          "500": {
            "description": "Internal server error",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
>>>>>>> integration-branch
                }
              }
            }
          }
<<<<<<< HEAD
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
      }
    },
    "/v1/adapters/{adapter_id}": {
      "get": {
        "tags": ["adapters"],
        "summary": "Get adapter details",
        "operationId": "get_adapter",
=======
        }
      },
      "post": {
        "tags": [
          "domain_adapters"
        ],
        "summary": "Create a new domain adapter",
        "operationId": "create_domain_adapter",
        "requestBody": {
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/CreateDomainAdapterRequest"
              }
            }
          },
          "required": true
        },
        "responses": {
          "201": {
            "description": "Domain adapter created",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/DomainAdapterResponse"
                }
              }
            }
          },
          "400": {
            "description": "Invalid request",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/domain-adapters/{adapter_id}": {
      "get": {
        "tags": [
          "domain_adapters"
        ],
        "summary": "Get a specific domain adapter",
        "operationId": "get_domain_adapter",
>>>>>>> integration-branch
        "parameters": [
          {
            "name": "adapter_id",
            "in": "path",
<<<<<<< HEAD
            "required": true,
            "schema": {
              "type": "string"
            },
            "description": "Adapter ID"
=======
            "description": "Domain adapter ID",
            "required": true,
            "schema": {
              "type": "string"
            }
>>>>>>> integration-branch
          }
        ],
        "responses": {
          "200": {
<<<<<<< HEAD
            "description": "Adapter details",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/AdapterDetails"
=======
            "description": "Domain adapter details",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/DomainAdapterResponse"
>>>>>>> integration-branch
                }
              }
            }
          },
          "404": {
<<<<<<< HEAD
            "description": "Adapter not found"
          }
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
      },
      "delete": {
        "tags": ["adapters"],
        "summary": "Delete adapter",
        "operationId": "delete_adapter",
=======
            "description": "Domain adapter not found",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          }
        }
      },
      "delete": {
        "tags": [
          "domain_adapters"
        ],
        "summary": "Delete a domain adapter",
        "operationId": "delete_domain_adapter",
>>>>>>> integration-branch
        "parameters": [
          {
            "name": "adapter_id",
            "in": "path",
<<<<<<< HEAD
            "required": true,
            "schema": {
              "type": "string"
            },
            "description": "Adapter ID"
=======
            "description": "Domain adapter ID",
            "required": true,
            "schema": {
              "type": "string"
            }
>>>>>>> integration-branch
          }
        ],
        "responses": {
          "204": {
<<<<<<< HEAD
            "description": "Adapter deleted"
          }
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
      }
    },
    "/v1/training/jobs": {
      "get": {
        "tags": ["training"],
        "summary": "List training jobs",
        "operationId": "list_training_jobs",
        "responses": {
          "200": {
            "description": "List of training jobs",
=======
            "description": "Domain adapter deleted"
          },
          "404": {
            "description": "Domain adapter not found",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/domain-adapters/{adapter_id}/execute": {
      "post": {
        "tags": [
          "domain_adapters"
        ],
        "summary": "Execute domain adapter with input data",
        "operationId": "execute_domain_adapter",
        "parameters": [
          {
            "name": "adapter_id",
            "in": "path",
            "description": "Domain adapter ID",
            "required": true,
            "schema": {
              "type": "string"
            }
          }
        ],
        "requestBody": {
          "content": {
            "application/json": {
              "schema": {}
            }
          },
          "required": true
        },
        "responses": {
          "200": {
            "description": "Execution completed",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/DomainAdapterExecutionResponse"
                }
              }
            }
          },
          "404": {
            "description": "Domain adapter not found",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/domain-adapters/{adapter_id}/load": {
      "post": {
        "tags": [
          "domain_adapters"
        ],
        "summary": "Load a domain adapter into the deterministic executor",
        "operationId": "load_domain_adapter",
        "parameters": [
          {
            "name": "adapter_id",
            "in": "path",
            "description": "Domain adapter ID",
            "required": true,
            "schema": {
              "type": "string"
            }
          }
        ],
        "requestBody": {
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/LoadDomainAdapterRequest"
              }
            }
          },
          "required": true
        },
        "responses": {
          "200": {
            "description": "Domain adapter loaded",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/DomainAdapterResponse"
                }
              }
            }
          },
          "404": {
            "description": "Domain adapter not found",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/domain-adapters/{adapter_id}/manifest": {
      "get": {
        "tags": [
          "domain_adapters"
        ],
        "summary": "Get domain adapter manifest",
        "operationId": "get_domain_adapter_manifest",
        "parameters": [
          {
            "name": "adapter_id",
            "in": "path",
            "description": "Domain adapter ID",
            "required": true,
            "schema": {
              "type": "string"
            }
          }
        ],
        "responses": {
          "200": {
            "description": "Domain adapter manifest",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/DomainAdapterManifestResponse"
                }
              }
            }
          },
          "404": {
            "description": "Domain adapter not found",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/domain-adapters/{adapter_id}/test": {
      "post": {
        "tags": [
          "domain_adapters"
        ],
        "summary": "Test a domain adapter for determinism",
        "operationId": "test_domain_adapter",
        "parameters": [
          {
            "name": "adapter_id",
            "in": "path",
            "description": "Domain adapter ID",
            "required": true,
            "schema": {
              "type": "string"
            }
          }
        ],
        "requestBody": {
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/TestDomainAdapterRequest"
              }
            }
          },
          "required": true
        },
        "responses": {
          "200": {
            "description": "Test completed",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/TestDomainAdapterResponse"
                }
              }
            }
          },
          "404": {
            "description": "Domain adapter not found",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/domain-adapters/{adapter_id}/unload": {
      "post": {
        "tags": [
          "domain_adapters"
        ],
        "summary": "Unload a domain adapter from the deterministic executor",
        "operationId": "unload_domain_adapter",
        "parameters": [
          {
            "name": "adapter_id",
            "in": "path",
            "description": "Domain adapter ID",
            "required": true,
            "schema": {
              "type": "string"
            }
          }
        ],
        "responses": {
          "200": {
            "description": "Domain adapter unloaded",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/DomainAdapterResponse"
                }
              }
            }
          },
          "404": {
            "description": "Domain adapter not found",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/git/branches": {
      "get": {
        "tags": [
          "git"
        ],
        "summary": "List adapter Git branches",
        "operationId": "list_git_branches",
        "responses": {
          "200": {
            "description": "List of adapter branches",
>>>>>>> integration-branch
            "content": {
              "application/json": {
                "schema": {
                  "type": "array",
                  "items": {
<<<<<<< HEAD
                    "$ref": "#/components/schemas/TrainingJobInfo"
=======
                    "$ref": "#/components/schemas/GitBranchInfo"
>>>>>>> integration-branch
                  }
                }
              }
            }
<<<<<<< HEAD
          }
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
      }
    },
    "/v1/training/start": {
      "post": {
        "tags": ["training"],
        "summary": "Start new training job",
        "operationId": "start_training",
        "requestBody": {
          "required": true,
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/StartTrainingRequest"
              }
            }
          }
        },
        "responses": {
          "201": {
            "description": "Training job started",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/TrainingJobResponse"
=======
          },
          "500": {
            "description": "Internal server error",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
>>>>>>> integration-branch
                }
              }
            }
          }
<<<<<<< HEAD
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
      }
    },
    "/v1/training/jobs/{job_id}": {
      "get": {
        "tags": ["training"],
        "summary": "Get training job status",
        "operationId": "get_training_job",
        "parameters": [
          {
            "name": "job_id",
            "in": "path",
            "required": true,
            "schema": {
              "type": "string"
            },
            "description": "Training job ID"
          }
        ],
        "responses": {
          "200": {
            "description": "Training job details",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/TrainingJobDetails"
=======
        }
      }
    },
    "/v1/git/sessions/start": {
      "post": {
        "tags": [
          "git"
        ],
        "summary": "Start a new Git session for an adapter",
        "operationId": "start_git_session",
        "requestBody": {
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/StartGitSessionRequest"
              }
            }
          },
          "required": true
        },
        "responses": {
          "200": {
            "description": "Session started",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/StartGitSessionResponse"
                }
              }
            }
          },
          "400": {
            "description": "Bad request",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          },
          "500": {
            "description": "Internal server error",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/git/sessions/{session_id}/end": {
      "post": {
        "tags": [
          "git"
        ],
        "summary": "End a Git session",
        "operationId": "end_git_session",
        "parameters": [
          {
            "name": "session_id",
            "in": "path",
            "description": "Git session ID",
            "required": true,
            "schema": {
              "type": "string"
            }
          }
        ],
        "requestBody": {
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/EndGitSessionRequest"
              }
            }
          },
          "required": true
        },
        "responses": {
          "200": {
            "description": "Session ended",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/EndGitSessionResponse"
>>>>>>> integration-branch
                }
              }
            }
          },
          "404": {
<<<<<<< HEAD
            "description": "Job not found"
          }
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
      },
      "delete": {
        "tags": ["training"],
        "summary": "Cancel training job",
        "operationId": "cancel_training",
        "parameters": [
          {
            "name": "job_id",
            "in": "path",
            "required": true,
            "schema": {
              "type": "string"
            },
            "description": "Training job ID"
          }
        ],
        "responses": {
          "200": {
            "description": "Job cancelled"
          }
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
      }
    },
    "/v1/chat/completions": {
      "post": {
        "tags": ["inference"],
        "summary": "Perform chat completion",
        "operationId": "chat_completion",
        "requestBody": {
          "required": true,
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/ChatCompletionRequest"
              }
            }
          }
        },
        "responses": {
          "200": {
            "description": "Chat completion response",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ChatCompletionResponse"
=======
            "description": "Session not found",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          },
          "500": {
            "description": "Internal server error",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
>>>>>>> integration-branch
                }
              }
            }
          }
<<<<<<< HEAD
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
=======
        }
      }
    },
    "/v1/git/status": {
      "get": {
        "tags": [
          "git"
        ],
        "summary": "Get Git status",
        "operationId": "git_status",
        "responses": {
          "200": {
            "description": "Git status",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/GitStatusResponse"
                }
              }
            }
          },
          "500": {
            "description": "Internal server error",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/infer": {
      "post": {
        "tags": [
          "handlers"
        ],
        "summary": "Inference endpoint",
        "operationId": "infer",
        "requestBody": {
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/InferRequest"
              }
            }
          },
          "required": true
        },
        "responses": {
          "200": {
            "description": "Inference successful",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/InferResponse"
                }
              }
            }
          },
          "400": {
            "description": "Invalid request",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          },
          "500": {
            "description": "Inference failed",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          },
          "501": {
            "description": "Worker not initialized",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/metrics/adapters": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "Get adapter performance metrics",
        "operationId": "get_adapter_metrics",
        "responses": {
          "200": {
            "description": "Adapter metrics",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/AdapterMetricsResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/metrics/quality": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "Get quality metrics",
        "operationId": "get_quality_metrics",
        "responses": {
          "200": {
            "description": "Quality metrics",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/QualityMetricsResponse"
                }
              }
            }
          }
        }
>>>>>>> integration-branch
      }
    },
    "/v1/metrics/system": {
      "get": {
<<<<<<< HEAD
        "tags": ["metrics"],
=======
        "tags": [
          "handlers"
        ],
>>>>>>> integration-branch
        "summary": "Get system metrics",
        "operationId": "get_system_metrics",
        "responses": {
          "200": {
            "description": "System metrics",
            "content": {
              "application/json": {
                "schema": {
<<<<<<< HEAD
                  "$ref": "#/components/schemas/SystemMetrics"
=======
                  "$ref": "#/components/schemas/SystemMetricsResponse"
>>>>>>> integration-branch
                }
              }
            }
          }
<<<<<<< HEAD
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
      }
    },
    "/v1/metrics/adapters/{adapter_id}": {
      "get": {
        "tags": ["metrics"],
        "summary": "Get adapter metrics",
        "operationId": "get_adapter_metrics",
        "parameters": [
          {
            "name": "adapter_id",
            "in": "path",
            "required": true,
            "schema": {
              "type": "string"
            },
            "description": "Adapter ID"
=======
        }
      }
    },
    "/v1/models/status": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "Get base model status",
        "operationId": "get_base_model_status",
        "parameters": [
          {
            "name": "tenant_id",
            "in": "query",
            "description": "Filter by tenant ID",
            "required": false,
            "schema": {
              "type": "string",
              "nullable": true
            }
>>>>>>> integration-branch
          }
        ],
        "responses": {
          "200": {
<<<<<<< HEAD
            "description": "Adapter metrics",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/AdapterMetrics"
=======
            "description": "Base model status",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/BaseModelStatusResponse"
                }
              }
            }
          },
          "404": {
            "description": "No base model status found",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
>>>>>>> integration-branch
                }
              }
            }
          }
<<<<<<< HEAD
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
=======
        }
>>>>>>> integration-branch
      }
    },
    "/v1/repositories": {
      "get": {
<<<<<<< HEAD
        "tags": ["repositories"],
=======
        "tags": [
          "handlers"
        ],
>>>>>>> integration-branch
        "summary": "List repositories",
        "operationId": "list_repositories",
        "responses": {
          "200": {
            "description": "List of repositories",
            "content": {
              "application/json": {
                "schema": {
                  "type": "array",
                  "items": {
<<<<<<< HEAD
                    "$ref": "#/components/schemas/RepositoryInfo"
=======
                    "$ref": "#/components/schemas/RepositoryResponse"
>>>>>>> integration-branch
                  }
                }
              }
            }
          }
<<<<<<< HEAD
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
=======
        }
      }
    },
    "/v1/repositories/register": {
      "post": {
        "tags": [
          "handlers"
        ],
        "summary": "Register repository",
        "operationId": "register_repository",
        "requestBody": {
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/RegisterRepositoryRequest"
              }
            }
          },
          "required": true
        },
        "responses": {
          "201": {
            "description": "Repository registered",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/RepositoryResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/repositories/{repo_id}/scan": {
      "post": {
        "tags": [
          "handlers"
        ],
        "summary": "Trigger repository scan",
        "operationId": "trigger_repository_scan",
        "parameters": [
          {
            "name": "repo_id",
            "in": "path",
            "description": "Repository ID",
            "required": true,
            "schema": {
              "type": "string"
            }
          }
        ],
        "responses": {
          "202": {
            "description": "Scan triggered",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ScanStatusResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/repositories/{repo_id}/status": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "Get repository scan status",
        "operationId": "get_repository_status",
        "parameters": [
          {
            "name": "repo_id",
            "in": "path",
            "description": "Repository ID",
            "required": true,
            "schema": {
              "type": "string"
            }
          }
        ],
        "responses": {
          "200": {
            "description": "Scan status",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ScanStatusResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/routing/debug": {
      "post": {
        "tags": [
          "handlers"
        ],
        "summary": "Debug routing decision",
        "operationId": "debug_routing",
        "requestBody": {
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/RoutingDebugRequest"
              }
            }
          },
          "required": true
        },
        "responses": {
          "200": {
            "description": "Routing debug info",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/RoutingDebugResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/routing/history": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "Get routing history",
        "operationId": "get_routing_history",
        "responses": {
          "200": {
            "description": "Routing history",
            "content": {
              "application/json": {
                "schema": {
                  "type": "array",
                  "items": {
                    "$ref": "#/components/schemas/RoutingDebugResponse"
                  }
                }
              }
            }
          }
        }
      }
    },
    "/v1/streams/contacts": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "Contacts stream SSE endpoint",
        "description": "Streams real-time contact discovery and update events as contacts are\ndiscovered during inference operations.\n\nCitation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §2.6",
        "operationId": "contacts_stream",
        "parameters": [
          {
            "name": "tenant",
            "in": "query",
            "description": "Tenant ID for filtering events",
            "required": true,
            "schema": {
              "type": "string"
            }
          }
        ],
        "responses": {
          "200": {
            "description": "SSE stream of contact events"
          }
        }
      }
    },
    "/v1/streams/discovery": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "Discovery stream SSE endpoint",
        "description": "Streams real-time repository scanning and code discovery events including\nscan progress, symbol indexing, framework detection, and completion events.\n\nEvents are sent as Server-Sent Events (SSE) with the following format:\n```\nevent: discovery\ndata: {\"type\":\"symbol_indexed\",\"timestamp\":...,\"payload\":{...}}\n```\n\nCitation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §4.4",
        "operationId": "discovery_stream",
        "parameters": [
          {
            "name": "tenant",
            "in": "query",
            "description": "Tenant ID for filtering events",
            "required": true,
            "schema": {
              "type": "string"
            }
          },
          {
            "name": "repo",
            "in": "query",
            "description": "Optional repository ID filter",
            "required": false,
            "schema": {
              "type": "string",
              "nullable": true
            }
          }
        ],
        "responses": {
          "200": {
            "description": "SSE stream of discovery events"
          }
        }
      }
    },
    "/v1/streams/file-changes": {
      "get": {
        "tags": [
          "git"
        ],
        "summary": "Stream file changes via SSE",
        "operationId": "file_changes_stream",
        "parameters": [
          {
            "name": "repo_id",
            "in": "query",
            "description": "Filter by repository ID",
            "required": false,
            "schema": {
              "type": "string",
              "nullable": true
            }
          }
        ],
        "responses": {
          "200": {
            "description": "Server-Sent Events stream of file changes"
          },
          "500": {
            "description": "Internal server error"
          }
        }
      }
    },
    "/v1/streams/training": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "Training stream SSE endpoint",
        "description": "Streams real-time training events including adapter lifecycle transitions,\npromotion/demotion events, profiler metrics, and K reduction events.\n\nEvents are sent as Server-Sent Events (SSE) with the following format:\n```\nevent: training\ndata: {\"type\":\"adapter_promoted\",\"timestamp\":...,\"payload\":{...}}\n```\n\nCitation: CONTACTS_AND_STREAMS_IMPLEMENTATION_PLAN.md §3.5",
        "operationId": "training_stream",
        "parameters": [
          {
            "name": "tenant",
            "in": "query",
            "description": "Tenant ID for filtering events",
            "required": true,
            "schema": {
              "type": "string"
            }
          }
        ],
        "responses": {
          "200": {
            "description": "SSE stream of training events"
          }
        }
      }
    },
    "/v1/training/jobs": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "List all training jobs",
        "operationId": "list_training_jobs",
        "responses": {
          "200": {
            "description": "Training jobs retrieved successfully",
            "content": {
              "application/json": {
                "schema": {
                  "type": "array",
                  "items": {
                    "$ref": "#/components/schemas/TrainingJobResponse"
                  }
                }
              }
            }
          }
        }
      }
    },
    "/v1/training/jobs/{job_id}": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "Get a specific training job",
        "operationId": "get_training_job",
        "parameters": [
          {
            "name": "job_id",
            "in": "path",
            "description": "Training job ID",
            "required": true,
            "schema": {
              "type": "string"
            }
          }
        ],
        "responses": {
          "200": {
            "description": "Training job retrieved successfully",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/TrainingJobResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/training/jobs/{job_id}/cancel": {
      "post": {
        "tags": [
          "handlers"
        ],
        "summary": "Cancel a training job",
        "operationId": "cancel_training",
        "parameters": [
          {
            "name": "job_id",
            "in": "path",
            "description": "Training job ID",
            "required": true,
            "schema": {
              "type": "string"
            }
          }
        ],
        "responses": {
          "200": {
            "description": "Training cancelled successfully"
          }
        }
      }
    },
    "/v1/training/jobs/{job_id}/logs": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "Get training logs",
        "operationId": "get_training_logs",
        "parameters": [
          {
            "name": "job_id",
            "in": "path",
            "description": "Training job ID",
            "required": true,
            "schema": {
              "type": "string"
            }
          }
        ],
        "responses": {
          "200": {
            "description": "Logs retrieved successfully",
            "content": {
              "application/json": {
                "schema": {
                  "type": "array",
                  "items": {
                    "type": "string"
                  }
                }
              }
            }
          }
        }
      }
    },
    "/v1/training/jobs/{job_id}/metrics": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "Get training metrics",
        "operationId": "get_training_metrics",
        "parameters": [
          {
            "name": "job_id",
            "in": "path",
            "description": "Training job ID",
            "required": true,
            "schema": {
              "type": "string"
            }
          }
        ],
        "responses": {
          "200": {
            "description": "Metrics retrieved successfully",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/TrainingMetricsResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/training/start": {
      "post": {
        "tags": [
          "handlers"
        ],
        "summary": "Start a new training job",
        "operationId": "start_training",
        "requestBody": {
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/StartTrainingRequest"
              }
            }
          },
          "required": true
        },
        "responses": {
          "200": {
            "description": "Training started successfully",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/TrainingJobResponse"
                }
              }
            }
          }
        }
      }
    },
    "/v1/training/templates": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "List training templates",
        "operationId": "list_training_templates",
        "responses": {
          "200": {
            "description": "Training templates retrieved successfully",
            "content": {
              "application/json": {
                "schema": {
                  "type": "array",
                  "items": {
                    "$ref": "#/components/schemas/TrainingTemplateResponse"
                  }
                }
              }
            }
          }
        }
      }
    },
    "/v1/training/templates/{template_id}": {
      "get": {
        "tags": [
          "handlers"
        ],
        "summary": "Get a specific training template",
        "operationId": "get_training_template",
        "parameters": [
          {
            "name": "template_id",
            "in": "path",
            "description": "Training template ID",
            "required": true,
            "schema": {
              "type": "string"
            }
          }
        ],
        "responses": {
          "200": {
            "description": "Training template retrieved successfully",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/TrainingTemplateResponse"
                }
              }
            }
          }
        }
>>>>>>> integration-branch
      }
    }
  },
  "components": {
<<<<<<< HEAD
    "securitySchemes": {
      "bearer_token": {
        "type": "http",
        "scheme": "bearer",
        "bearerFormat": "JWT"
      }
    },
    "schemas": {
      "ErrorResponse": {
        "type": "object",
        "properties": {
          "error": {
            "type": "string"
          },
=======
    "schemas": {
      "AdapterActivationResponse": {
        "type": "object",
        "description": "Adapter activation response",
        "required": [
          "id",
          "adapter_id",
          "gate_value",
          "selected",
          "created_at"
        ],
        "properties": {
          "adapter_id": {
            "type": "string"
          },
          "created_at": {
            "type": "string"
          },
          "gate_value": {
            "type": "number",
            "format": "double"
          },
          "id": {
            "type": "string"
          },
          "request_id": {
            "type": "string",
            "nullable": true
          },
          "selected": {
            "type": "boolean"
          }
        }
      },
      "AdapterMetricsResponse": {
        "type": "object",
        "description": "Adapter metrics response",
        "required": [
          "adapters"
        ],
        "properties": {
          "adapters": {
            "type": "array",
            "items": {
              "$ref": "#/components/schemas/AdapterPerformance"
            }
          }
        }
      },
      "AdapterPerformance": {
        "type": "object",
        "description": "Adapter performance metrics",
        "required": [
          "adapter_id",
          "name",
          "activation_rate",
          "avg_gate_value",
          "total_requests"
        ],
        "properties": {
          "activation_rate": {
            "type": "number",
            "format": "double"
          },
          "adapter_id": {
            "type": "string"
          },
          "avg_gate_value": {
            "type": "number",
            "format": "double"
          },
          "name": {
            "type": "string"
          },
          "total_requests": {
            "type": "integer",
            "format": "int64"
          }
        }
      },
      "AdapterResponse": {
        "type": "object",
        "description": "Adapter response",
        "required": [
          "id",
          "adapter_id",
          "name",
          "hash_b3",
          "rank",
          "tier",
          "languages",
          "created_at"
        ],
        "properties": {
          "adapter_id": {
            "type": "string"
          },
          "created_at": {
            "type": "string"
          },
          "framework": {
            "type": "string",
            "nullable": true
          },
          "hash_b3": {
            "type": "string"
          },
          "id": {
            "type": "string"
          },
          "languages": {
            "type": "array",
            "items": {
              "type": "string"
            }
          },
          "name": {
            "type": "string"
          },
          "rank": {
            "type": "integer",
            "format": "int32"
          },
          "stats": {
            "allOf": [
              {
                "$ref": "#/components/schemas/AdapterStats"
              }
            ],
            "nullable": true
          },
          "tier": {
            "type": "integer",
            "format": "int32"
          }
        }
      },
      "AdapterScore": {
        "type": "object",
        "description": "Adapter score",
        "required": [
          "adapter_id",
          "score",
          "gate_value",
          "selected"
        ],
        "properties": {
          "adapter_id": {
            "type": "string"
          },
          "gate_value": {
            "type": "number",
            "format": "double"
          },
          "score": {
            "type": "number",
            "format": "double"
          },
          "selected": {
            "type": "boolean"
          }
        }
      },
      "AdapterStats": {
        "type": "object",
        "description": "Adapter statistics",
        "required": [
          "total_activations",
          "selected_count",
          "avg_gate_value",
          "selection_rate"
        ],
        "properties": {
          "avg_gate_value": {
            "type": "number",
            "format": "double"
          },
          "selected_count": {
            "type": "integer",
            "format": "int64"
          },
          "selection_rate": {
            "type": "number",
            "format": "double"
          },
          "total_activations": {
            "type": "integer",
            "format": "int64"
          }
        }
      },
      "AuditExtended": {
        "type": "object",
        "description": "Extended audit record with before/after CPID",
        "required": [
          "id",
          "tenant_id",
          "cpid",
          "created_at"
        ],
        "properties": {
          "after_cpid": {
            "type": "string",
            "nullable": true
          },
          "arr": {
            "type": "number",
            "format": "double",
            "nullable": true
          },
          "before_cpid": {
            "type": "string",
            "nullable": true
          },
          "cpid": {
            "type": "string"
          },
          "cr": {
            "type": "number",
            "format": "double",
            "nullable": true
          },
          "created_at": {
            "type": "string"
          },
          "ecs5": {
            "type": "number",
            "format": "double",
            "nullable": true
          },
          "hlr": {
            "type": "number",
            "format": "double",
            "nullable": true
          },
          "id": {
            "type": "string"
          },
          "status": {
            "type": "string",
            "nullable": true
          },
          "tenant_id": {
            "type": "string"
          }
        }
      },
      "AuditsQuery": {
        "type": "object",
        "description": "Audits query parameters",
        "required": [
          "tenant"
        ],
        "properties": {
          "limit": {
            "type": "integer",
            "nullable": true,
            "minimum": 0
          },
          "tenant": {
            "type": "string"
          }
        }
      },
      "AuditsResponse": {
        "type": "object",
        "description": "Audits response",
        "required": [
          "items"
        ],
        "properties": {
          "items": {
            "type": "array",
            "items": {
              "$ref": "#/components/schemas/AuditExtended"
            }
          }
        }
      },
      "BaseModelStatusResponse": {
        "type": "object",
        "description": "Base model status response",
        "required": [
          "model_id",
          "model_name",
          "status",
          "is_loaded",
          "updated_at"
        ],
        "properties": {
          "error_message": {
            "type": "string",
            "nullable": true
          },
          "is_loaded": {
            "type": "boolean"
          },
          "loaded_at": {
            "type": "string",
            "nullable": true
          },
          "memory_usage_mb": {
            "type": "integer",
            "format": "int32",
            "nullable": true
          },
          "model_id": {
            "type": "string"
          },
          "model_name": {
            "type": "string"
          },
          "status": {
            "type": "string"
          },
          "unloaded_at": {
            "type": "string",
            "nullable": true
          },
          "updated_at": {
            "type": "string"
          }
        }
      },
      "CommitDiffResponse": {
        "type": "object",
        "description": "Commit diff response",
        "required": [
          "sha",
          "diff",
          "stats"
        ],
        "properties": {
          "diff": {
            "type": "string"
          },
          "sha": {
            "type": "string"
          },
          "stats": {
            "$ref": "#/components/schemas/DiffStats"
          }
        }
      },
      "CommitResponse": {
        "type": "object",
        "description": "Commit response",
        "required": [
          "id",
          "repo_id",
          "sha",
          "author",
          "date",
          "message",
          "changed_files",
          "impacted_symbols"
        ],
        "properties": {
          "author": {
            "type": "string"
          },
          "branch": {
            "type": "string",
            "nullable": true
          },
          "changed_files": {
            "type": "array",
            "items": {
              "type": "string"
            }
          },
          "date": {
            "type": "string"
          },
          "ephemeral_adapter_id": {
            "type": "string",
            "nullable": true
          },
          "id": {
            "type": "string"
          },
          "impacted_symbols": {
            "type": "array",
            "items": {
              "type": "string"
            }
          },
          "message": {
            "type": "string"
          },
          "repo_id": {
            "type": "string"
          },
          "sha": {
            "type": "string"
          }
        }
      },
      "ContactInteractionResponse": {
        "type": "object",
        "description": "Contact interaction response",
        "required": [
          "id",
          "contact_id",
          "trace_id",
          "cpid",
          "interaction_type",
          "created_at"
        ],
        "properties": {
          "contact_id": {
            "type": "string"
          },
          "context_json": {
            "type": "string",
            "nullable": true
          },
          "cpid": {
            "type": "string"
          },
          "created_at": {
            "type": "string"
          },
          "id": {
            "type": "string"
          },
          "interaction_type": {
            "type": "string"
          },
          "trace_id": {
            "type": "string"
          }
        }
      },
      "ContactInteractionsResponse": {
        "type": "object",
        "description": "Contact interactions list response",
        "required": [
          "interactions"
        ],
        "properties": {
          "interactions": {
            "type": "array",
            "items": {
              "$ref": "#/components/schemas/ContactInteractionResponse"
            }
          }
        }
      },
      "ContactResponse": {
        "type": "object",
        "description": "Contact response",
        "required": [
          "id",
          "tenant_id",
          "name",
          "category",
          "discovered_at",
          "interaction_count",
          "created_at",
          "updated_at"
        ],
        "properties": {
          "avatar_url": {
            "type": "string",
            "nullable": true
          },
          "category": {
            "type": "string"
          },
          "created_at": {
            "type": "string"
          },
          "discovered_at": {
            "type": "string"
          },
          "discovered_by": {
            "type": "string",
            "nullable": true
          },
          "email": {
            "type": "string",
            "nullable": true
          },
          "id": {
            "type": "string"
          },
          "interaction_count": {
            "type": "integer",
            "format": "int32"
          },
          "last_interaction": {
            "type": "string",
            "nullable": true
          },
          "metadata_json": {
            "type": "string",
            "nullable": true
          },
          "name": {
            "type": "string"
          },
          "role": {
            "type": "string",
            "nullable": true
          },
          "tenant_id": {
            "type": "string"
          },
          "updated_at": {
            "type": "string"
          }
        }
      },
      "ContactsResponse": {
        "type": "object",
        "description": "Contacts list response",
        "required": [
          "contacts"
        ],
        "properties": {
          "contacts": {
            "type": "array",
            "items": {
              "$ref": "#/components/schemas/ContactResponse"
            }
          }
        }
      },
      "CreateContactRequest": {
        "type": "object",
        "description": "Create contact request",
        "required": [
          "tenant_id",
          "name",
          "category"
        ],
        "properties": {
          "category": {
            "type": "string"
          },
          "email": {
            "type": "string",
            "nullable": true
          },
          "metadata_json": {
            "type": "string",
            "nullable": true
          },
          "name": {
            "type": "string"
          },
          "role": {
            "type": "string",
            "nullable": true
          },
          "tenant_id": {
            "type": "string"
          }
        }
      },
      "CreateDomainAdapterRequest": {
        "type": "object",
        "description": "Create domain adapter request",
        "required": [
          "name",
          "version",
          "description",
          "domain_type",
          "model",
          "hash",
          "input_format",
          "output_format",
          "config"
        ],
        "properties": {
          "config": {
            "type": "object",
            "additionalProperties": {}
          },
          "description": {
            "type": "string"
          },
          "domain_type": {
            "type": "string"
          },
          "hash": {
            "type": "string"
          },
          "input_format": {
            "type": "string"
          },
          "model": {
            "type": "string"
          },
          "name": {
            "type": "string"
          },
          "output_format": {
            "type": "string"
          },
          "version": {
            "type": "string"
          }
        }
      },
      "CreateTenantRequest": {
        "type": "object",
        "description": "Create tenant request",
        "required": [
          "name",
          "itar_flag"
        ],
        "properties": {
          "itar_flag": {
            "type": "boolean"
          },
          "name": {
            "type": "string"
          }
        }
      },
      "DiffStats": {
        "type": "object",
        "description": "Diff statistics",
        "required": [
          "files_changed",
          "insertions",
          "deletions"
        ],
        "properties": {
          "deletions": {
            "type": "integer",
            "format": "int32"
          },
          "files_changed": {
            "type": "integer",
            "format": "int32"
          },
          "insertions": {
            "type": "integer",
            "format": "int32"
          }
        }
      },
      "DiscoveryStreamQuery": {
        "type": "object",
        "description": "Discovery stream query parameters",
        "required": [
          "tenant"
        ],
        "properties": {
          "repo": {
            "type": "string",
            "nullable": true
          },
          "tenant": {
            "type": "string"
          }
        }
      },
      "DomainAdapterExecutionResponse": {
        "type": "object",
        "description": "Domain adapter execution response",
        "required": [
          "execution_id",
          "adapter_id",
          "input_hash",
          "output_hash",
          "epsilon",
          "execution_time_ms",
          "trace_events",
          "executed_at"
        ],
        "properties": {
          "adapter_id": {
            "type": "string"
          },
          "epsilon": {
            "type": "number",
            "format": "double"
          },
          "executed_at": {
            "type": "string"
          },
          "execution_id": {
            "type": "string"
          },
          "execution_time_ms": {
            "type": "integer",
            "format": "int64",
            "minimum": 0
          },
          "input_hash": {
            "type": "string"
          },
          "output_hash": {
            "type": "string"
          },
          "trace_events": {
            "type": "array",
            "items": {
              "type": "string"
            }
          }
        }
      },
      "DomainAdapterManifestResponse": {
        "type": "object",
        "description": "Domain adapter manifest response",
        "required": [
          "adapter_id",
          "name",
          "version",
          "description",
          "domain_type",
          "model",
          "hash",
          "input_format",
          "output_format",
          "config",
          "created_at",
          "updated_at"
        ],
        "properties": {
          "adapter_id": {
            "type": "string"
          },
          "config": {
            "type": "object",
            "additionalProperties": {}
          },
          "created_at": {
            "type": "string"
          },
          "description": {
            "type": "string"
          },
          "domain_type": {
            "type": "string"
          },
          "hash": {
            "type": "string"
          },
          "input_format": {
            "type": "string"
          },
          "model": {
            "type": "string"
          },
          "name": {
            "type": "string"
          },
          "output_format": {
            "type": "string"
          },
          "updated_at": {
            "type": "string"
          },
          "version": {
            "type": "string"
          }
        }
      },
      "DomainAdapterResponse": {
        "type": "object",
        "description": "Domain adapter response",
        "required": [
          "id",
          "name",
          "version",
          "description",
          "domain_type",
          "model",
          "hash",
          "input_format",
          "output_format",
          "config",
          "status",
          "execution_count",
          "created_at",
          "updated_at"
        ],
        "properties": {
          "config": {
            "type": "object",
            "additionalProperties": {}
          },
          "created_at": {
            "type": "string"
          },
          "description": {
            "type": "string"
          },
          "domain_type": {
            "type": "string"
          },
          "epsilon_stats": {
            "allOf": [
              {
                "$ref": "#/components/schemas/EpsilonStatsResponse"
              }
            ],
            "nullable": true
          },
          "execution_count": {
            "type": "integer",
            "format": "int64",
            "minimum": 0
          },
          "hash": {
            "type": "string"
          },
          "id": {
            "type": "string"
          },
          "input_format": {
            "type": "string"
          },
          "last_execution": {
            "type": "string",
            "nullable": true
          },
          "model": {
            "type": "string"
          },
          "name": {
            "type": "string"
          },
          "output_format": {
            "type": "string"
          },
          "status": {
            "type": "string"
          },
          "updated_at": {
            "type": "string"
          },
          "version": {
            "type": "string"
          }
        }
      },
      "EndGitSessionRequest": {
        "type": "object",
        "description": "End Git session request",
        "required": [
          "action"
        ],
        "properties": {
          "action": {
            "$ref": "#/components/schemas/SessionAction"
          }
        }
      },
      "EndGitSessionResponse": {
        "type": "object",
        "description": "End Git session response",
        "required": [
          "status"
        ],
        "properties": {
          "merge_commit_sha": {
            "type": "string",
            "nullable": true
          },
          "status": {
            "type": "string"
          }
        }
      },
      "EpsilonStatsResponse": {
        "type": "object",
        "description": "Epsilon statistics response",
        "required": [
          "mean_error",
          "max_error",
          "error_count",
          "last_updated"
        ],
        "properties": {
          "error_count": {
            "type": "integer",
            "format": "int64",
            "minimum": 0
          },
          "last_updated": {
            "type": "string"
          },
          "max_error": {
            "type": "number",
            "format": "double"
          },
          "mean_error": {
            "type": "number",
            "format": "double"
          }
        }
      },
      "ErrorResponse": {
        "type": "object",
        "description": "API error response",
        "required": [
          "error"
        ],
        "properties": {
>>>>>>> integration-branch
          "code": {
            "type": "string"
          },
          "details": {
<<<<<<< HEAD
            "type": "object"
          }
        },
        "required": ["error", "code"]
      },
      "HealthResponse": {
        "type": "object",
        "properties": {
          "status": {
            "type": "string",
            "enum": ["healthy", "unhealthy"]
          },
          "timestamp": {
            "type": "string",
            "format": "date-time"
          }
        }
      },
      "LoginRequest": {
        "type": "object",
        "properties": {
          "email": {
            "type": "string",
            "format": "email"
          },
          "password": {
            "type": "string"
          }
        },
        "required": ["email", "password"]
      },
      "LoginResponse": {
        "type": "object",
        "properties": {
          "token": {
            "type": "string"
          },
          "expires_at": {
            "type": "string",
            "format": "date-time"
          },
          "user": {
            "$ref": "#/components/schemas/UserInfo"
          }
        }
      },
      "UserInfo": {
        "type": "object",
        "properties": {
          "id": {
            "type": "string"
          },
          "email": {
            "type": "string",
            "format": "email"
          },
          "role": {
            "type": "string",
            "enum": ["admin", "operator", "user"]
          }
        }
      },
      "ModelInfo": {
        "type": "object",
        "properties": {
          "id": {
            "type": "string"
          },
          "name": {
            "type": "string"
          },
          "base_model": {
            "type": "string"
          },
          "status": {
            "type": "string",
            "enum": ["available", "loading", "unloading", "error"]
          },
          "created_at": {
            "type": "string",
            "format": "date-time"
          }
        }
      },
      "ImportModelRequest": {
        "type": "object",
        "properties": {
          "name": {
            "type": "string"
          },
          "weights_path": {
            "type": "string"
          },
          "config_path": {
            "type": "string"
          },
          "tokenizer_path": {
            "type": "string"
          }
        },
        "required": ["name", "weights_path", "config_path", "tokenizer_path"]
      },
      "ImportModelResponse": {
        "type": "object",
        "properties": {
          "import_id": {
            "type": "string"
          },
          "status": {
            "type": "string",
            "enum": ["started", "completed", "failed"]
          }
        }
      },
      "ModelDetails": {
        "type": "object",
        "properties": {
          "id": {
            "type": "string"
          },
          "name": {
            "type": "string"
          },
          "base_model": {
            "type": "string"
          },
          "status": {
            "type": "string"
          },
          "metrics": {
            "$ref": "#/components/schemas/ModelMetrics"
          },
          "created_at": {
            "type": "string",
            "format": "date-time"
          }
        }
      },
      "ModelMetrics": {
        "type": "object",
        "properties": {
          "total_requests": {
            "type": "integer"
          },
          "avg_latency_ms": {
            "type": "number"
          },
          "error_rate": {
            "type": "number"
          }
        }
      },
      "AdapterInfo": {
        "type": "object",
        "properties": {
          "id": {
            "type": "string"
          },
          "name": {
            "type": "string"
          },
          "description": {
            "type": "string"
          },
          "rank": {
            "type": "integer"
          },
          "tags": {
            "type": "array",
            "items": {
              "type": "string"
            }
          },
          "created_at": {
            "type": "string",
            "format": "date-time"
          }
        }
      },
      "RegisterAdapterRequest": {
        "type": "object",
        "properties": {
          "manifest": {
            "$ref": "#/components/schemas/AdapterManifest"
          }
        },
        "required": ["manifest"]
      },
      "AdapterManifest": {
        "type": "object",
        "properties": {
          "name": {
            "type": "string"
          },
          "description": {
            "type": "string"
          },
          "base_model": {
            "type": "string"
          },
          "rank": {
            "type": "integer"
          },
          "tags": {
            "type": "array",
            "items": {
              "type": "string"
            }
          },
          "metadata": {
            "type": "object"
          }
        },
        "required": ["name", "base_model", "rank"]
      },
      "AdapterRegistrationResponse": {
        "type": "object",
        "properties": {
          "id": {
            "type": "string"
          },
          "status": {
            "type": "string"
          },
          "upload_url": {
=======
            "nullable": true
          },
          "error": {
            "type": "string"
          }
        }
      },
      "FeatureVector": {
        "type": "object",
        "description": "Feature vector",
        "required": [
          "frameworks",
          "symbol_hits",
          "path_tokens",
          "verb"
        ],
        "properties": {
          "frameworks": {
            "type": "array",
            "items": {
              "type": "string"
            }
          },
          "language": {
            "type": "string",
            "nullable": true
          },
          "path_tokens": {
            "type": "array",
            "items": {
              "type": "string"
            }
          },
          "symbol_hits": {
            "type": "integer",
            "format": "int32"
          },
          "verb": {
            "type": "string"
          }
        }
      },
      "FileChangeEvent": {
        "type": "object",
        "description": "File change event for SSE",
        "required": [
          "file_path",
          "change_type",
          "timestamp"
        ],
        "properties": {
          "adapter_id": {
            "type": "string",
            "nullable": true
          },
          "change_type": {
            "type": "string"
          },
          "file_path": {
            "type": "string"
          },
          "timestamp": {
            "type": "string"
          }
        }
      },
      "GitBranchInfo": {
        "type": "object",
        "description": "Git branch info",
        "required": [
          "adapter_id",
          "branch_name",
          "created_at",
          "commit_count"
        ],
        "properties": {
          "adapter_id": {
            "type": "string"
          },
          "branch_name": {
            "type": "string"
          },
          "commit_count": {
            "type": "integer",
            "format": "int64"
          },
          "created_at": {
            "type": "string"
          }
        }
      },
      "GitStatusResponse": {
        "type": "object",
        "description": "Git status response",
        "required": [
          "branch",
          "modified_files",
          "untracked_files",
          "staged_files"
        ],
        "properties": {
          "branch": {
            "type": "string"
          },
          "modified_files": {
            "type": "array",
            "items": {
              "type": "string"
            }
          },
          "staged_files": {
            "type": "array",
            "items": {
              "type": "string"
            }
          },
          "untracked_files": {
            "type": "array",
            "items": {
              "type": "string"
            }
          }
        }
      },
      "HealthResponse": {
        "type": "object",
        "description": "Health check response",
        "required": [
          "status",
          "version"
        ],
        "properties": {
          "status": {
            "type": "string"
          },
          "version": {
>>>>>>> integration-branch
            "type": "string"
          }
        }
      },
<<<<<<< HEAD
      "AdapterDetails": {
        "type": "object",
        "properties": {
          "id": {
            "type": "string"
          },
          "manifest": {
            "$ref": "#/components/schemas/AdapterManifest"
          },
          "metrics": {
            "$ref": "#/components/schemas/AdapterMetrics"
          },
          "status": {
            "type": "string"
          }
        }
      },
      "AdapterMetrics": {
        "type": "object",
        "properties": {
          "activation_count": {
            "type": "integer"
          },
          "success_rate": {
            "type": "number"
          },
          "avg_latency_ms": {
            "type": "number"
          },
          "memory_usage_mb": {
            "type": "number"
          }
        }
      },
      "TrainingJobInfo": {
        "type": "object",
        "properties": {
          "id": {
            "type": "string"
          },
          "name": {
            "type": "string"
          },
          "status": {
            "type": "string",
            "enum": ["queued", "running", "completed", "failed", "cancelled"]
          },
          "progress": {
            "type": "number",
            "minimum": 0,
            "maximum": 1
          },
          "created_at": {
            "type": "string",
            "format": "date-time"
          }
        }
      },
      "StartTrainingRequest": {
        "type": "object",
        "properties": {
          "name": {
            "type": "string"
          },
          "base_model": {
            "type": "string"
          },
          "dataset": {
            "$ref": "#/components/schemas/TrainingDataset"
          },
          "config": {
            "$ref": "#/components/schemas/TrainingConfig"
          }
        },
        "required": ["name", "base_model", "dataset", "config"]
      },
      "TrainingDataset": {
        "type": "object",
        "properties": {
          "path": {
            "type": "string"
          },
          "format": {
            "type": "string",
            "enum": ["jsonl", "json", "txt"]
          },
          "size": {
            "type": "integer"
          }
        }
      },
      "TrainingConfig": {
        "type": "object",
        "properties": {
          "rank": {
            "type": "integer"
          },
          "epochs": {
            "type": "integer"
          },
          "batch_size": {
            "type": "integer"
          },
          "learning_rate": {
            "type": "number"
          }
        }
      },
      "TrainingJobResponse": {
        "type": "object",
        "properties": {
          "id": {
            "type": "string"
          },
          "status": {
            "type": "string"
          },
          "queue_position": {
            "type": "integer"
          }
        }
      },
      "TrainingJobDetails": {
        "type": "object",
        "properties": {
          "id": {
            "type": "string"
          },
          "name": {
            "type": "string"
          },
          "status": {
            "type": "string"
          },
          "progress": {
            "type": "number"
          },
          "current_epoch": {
            "type": "integer"
          },
          "metrics": {
            "type": "object",
            "properties": {
              "loss": {
                "type": "number"
              },
              "perplexity": {
                "type": "number"
              },
              "learning_rate": {
                "type": "number"
              }
            }
          },
          "time_remaining": {
            "type": "string"
          }
        }
      },
      "ChatCompletionRequest": {
        "type": "object",
        "properties": {
          "prompt": {
            "type": "string"
          },
          "adapters": {
            "type": "array",
            "items": {
              "type": "string"
            }
          },
          "max_tokens": {
            "type": "integer",
            "default": 200
          },
          "temperature": {
            "type": "number",
            "default": 0.7
          },
          "stream": {
            "type": "boolean",
            "default": false
          }
        },
        "required": ["prompt"]
      },
      "ChatCompletionResponse": {
        "type": "object",
        "properties": {
          "id": {
            "type": "string"
          },
          "response": {
            "type": "string"
          },
          "usage": {
            "$ref": "#/components/schemas/TokenUsage"
          },
=======
      "InferRequest": {
        "type": "object",
        "description": "Inference request",
        "required": [
          "prompt"
        ],
        "properties": {
          "max_tokens": {
            "type": "integer",
            "nullable": true,
            "minimum": 0
          },
          "prompt": {
            "type": "string"
          },
          "require_evidence": {
            "type": "boolean",
            "nullable": true
          },
          "seed": {
            "type": "integer",
            "format": "int64",
            "nullable": true,
            "minimum": 0
          },
          "temperature": {
            "type": "number",
            "format": "float",
            "nullable": true
          },
          "top_k": {
            "type": "integer",
            "nullable": true,
            "minimum": 0
          },
          "top_p": {
            "type": "number",
            "format": "float",
            "nullable": true
          }
        }
      },
      "InferResponse": {
        "type": "object",
        "description": "Inference response",
        "required": [
          "text",
          "tokens",
          "finish_reason",
          "trace"
        ],
        "properties": {
          "finish_reason": {
            "type": "string"
          },
          "text": {
            "type": "string"
          },
          "tokens": {
            "type": "array",
            "items": {
              "type": "integer",
              "format": "int32",
              "minimum": 0
            }
          },
          "trace": {
            "$ref": "#/components/schemas/InferenceTrace"
          }
        }
      },
      "InferenceTrace": {
        "type": "object",
        "description": "Inference trace for observability",
        "required": [
          "adapters_used",
          "router_decisions",
          "latency_ms"
        ],
        "properties": {
>>>>>>> integration-branch
          "adapters_used": {
            "type": "array",
            "items": {
              "type": "string"
            }
          },
          "latency_ms": {
<<<<<<< HEAD
            "type": "number"
          }
        }
      },
      "TokenUsage": {
        "type": "object",
        "properties": {
          "prompt_tokens": {
            "type": "integer"
          },
          "completion_tokens": {
            "type": "integer"
          },
          "total_tokens": {
            "type": "integer"
          }
        }
      },
      "SystemMetrics": {
        "type": "object",
        "properties": {
          "memory": {
            "$ref": "#/components/schemas/MemoryMetrics"
          },
          "inference": {
            "$ref": "#/components/schemas/InferenceMetrics"
          },
          "adapters": {
            "$ref": "#/components/schemas/AdapterSystemMetrics"
          }
        }
      },
      "MemoryMetrics": {
        "type": "object",
        "properties": {
          "used_bytes": {
            "type": "integer"
          },
          "total_bytes": {
            "type": "integer"
          },
          "headroom_pct": {
            "type": "number"
          }
        }
      },
      "InferenceMetrics": {
        "type": "object",
        "properties": {
          "active_requests": {
            "type": "integer"
          },
          "queue_depth": {
            "type": "integer"
          },
          "avg_latency_ms": {
            "type": "number"
          },
          "tokens_per_sec": {
            "type": "number"
          }
        }
      },
      "AdapterSystemMetrics": {
        "type": "object",
        "properties": {
          "loaded_count": {
            "type": "integer"
          },
          "total_registered": {
            "type": "integer"
          },
          "memory_usage_bytes": {
            "type": "integer"
          }
        }
      },
      "RepositoryInfo": {
        "type": "object",
        "properties": {
          "id": {
            "type": "string"
          },
          "name": {
            "type": "string"
          },
          "url": {
            "type": "string"
          },
          "branch": {
            "type": "string"
          },
          "last_scanned": {
            "type": "string",
            "format": "date-time"
          },
          "file_count": {
            "type": "integer"
=======
            "type": "integer",
            "format": "int64",
            "minimum": 0
          },
          "router_decisions": {
            "type": "array",
            "items": {
              "$ref": "#/components/schemas/RouterDecision"
            }
          }
        }
      },
      "LoadAverageResponse": {
        "type": "object",
        "required": [
          "load_1min",
          "load_5min",
          "load_15min"
        ],
        "properties": {
          "load_15min": {
            "type": "number",
            "format": "double"
          },
          "load_1min": {
            "type": "number",
            "format": "double"
          },
          "load_5min": {
            "type": "number",
            "format": "double"
          }
        }
      },
      "LoadDomainAdapterRequest": {
        "type": "object",
        "description": "Load domain adapter request",
        "required": [
          "adapter_id"
        ],
        "properties": {
          "adapter_id": {
            "type": "string"
          },
          "executor_config": {
            "type": "object",
            "additionalProperties": {},
            "nullable": true
          }
        }
      },
      "LoginRequest": {
        "type": "object",
        "description": "Login request",
        "required": [
          "email",
          "password"
        ],
        "properties": {
          "email": {
            "type": "string"
          },
          "password": {
            "type": "string"
          }
        }
      },
      "LoginResponse": {
        "type": "object",
        "description": "Login response with JWT token",
        "required": [
          "token",
          "user_id",
          "role"
        ],
        "properties": {
          "role": {
            "type": "string"
          },
          "token": {
            "type": "string"
          },
          "user_id": {
            "type": "string"
          }
        }
      },
      "MetaResponse": {
        "type": "object",
        "description": "Meta information response",
        "required": [
          "version",
          "build_hash",
          "build_date"
        ],
        "properties": {
          "build_date": {
            "type": "string"
          },
          "build_hash": {
            "type": "string"
          },
          "version": {
            "type": "string"
          }
        }
      },
      "PromotionRecord": {
        "type": "object",
        "description": "Promotion record with signature",
        "required": [
          "id",
          "cpid",
          "promoted_by",
          "promoted_at",
          "signature_b64",
          "signer_key_id",
          "quality_json"
        ],
        "properties": {
          "before_cpid": {
            "type": "string",
            "nullable": true
          },
          "cpid": {
            "type": "string"
          },
          "id": {
            "type": "string"
          },
          "promoted_at": {
            "type": "string"
          },
          "promoted_by": {
            "type": "string"
          },
          "quality_json": {
            "type": "string"
          },
          "signature_b64": {
            "type": "string"
          },
          "signer_key_id": {
            "type": "string"
          }
        }
      },
      "ProposePatchRequest": {
        "type": "object",
        "description": "Propose patch request",
        "required": [
          "repo_id",
          "commit_sha",
          "description",
          "target_files"
        ],
        "properties": {
          "commit_sha": {
            "type": "string"
          },
          "description": {
            "type": "string"
          },
          "repo_id": {
            "type": "string"
          },
          "target_files": {
            "type": "array",
            "items": {
              "type": "string"
            }
          }
        }
      },
      "ProposePatchResponse": {
        "type": "object",
        "description": "Propose patch response",
        "required": [
          "proposal_id",
          "status",
          "message"
        ],
        "properties": {
          "message": {
            "type": "string"
          },
          "proposal_id": {
            "type": "string"
>>>>>>> integration-branch
          },
          "status": {
            "type": "string"
          }
        }
<<<<<<< HEAD
      }
    }
  }
}
```

## API Examples

This section provides comprehensive examples for all major AdapterOS API endpoints. All examples use the demo credentials (`admin@example.com` / `password`) unless otherwise noted.

### Authentication

#### Login
```bash
# Login to get JWT token
curl -X POST http://localhost:8080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "email": "admin@example.com",
    "password": "password"
  }'

# Response:
# {
#   "token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJFZERTQSJ9...",
#   "expires_at": "2025-11-03T20:00:00Z",
#   "user": {
#     "id": "admin",
#     "email": "admin@example.com",
#     "role": "admin"
#   }
# }
```

#### Using JWT Tokens
```bash
# Store token for reuse
TOKEN=$(curl -s -X POST http://localhost:8080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@example.com","password":"password"}' \
  | jq -r '.token')

# Use token in subsequent requests
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/api/v1/adapters
```

### Adapter Management

#### List All Adapters
```bash
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/api/v1/adapters

# Response: Array of adapter metadata
# [
#   {
#     "id": "code_lang_v1",
#     "name": "Code Language Adapter",
#     "description": "Specialized for code-related tasks",
#     "created_at": "2025-01-15T10:00:00Z",
#     "size_bytes": 52428800,
#     "rank": 16,
#     "tags": ["code", "programming"]
#   }
# ]
```

#### Get Specific Adapter
```bash
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/api/v1/adapters/code_lang_v1

# Response: Detailed adapter info
# {
#   "id": "code_lang_v1",
#   "manifest": {
#     "name": "Code Language Adapter",
#     "description": "Fine-tuned for code generation and analysis",
#     "base_model": "qwen2.5-7b-instruct",
#     "rank": 16,
#     "created_at": "2025-01-15T10:00:00Z"
#   },
#   "metrics": {
#     "total_activations": 1250,
#     "avg_latency_ms": 45.2,
#     "success_rate": 0.987
#   }
# }
```

#### Register New Adapter
```bash
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "manifest": {
      "name": "Custom Code Adapter",
      "description": "Fine-tuned for Python development",
      "base_model": "qwen2.5-7b-instruct",
      "rank": 16,
      "tags": ["python", "development"],
      "metadata": {
        "training_dataset": "python-code-corpus-v2",
        "epochs": 3,
        "learning_rate": 0.0001
      }
    }
  }' \
  http://localhost:8080/api/v1/adapters

# Response:
# {
#   "id": "custom_code_adapter_abc123",
#   "status": "registered",
#   "upload_url": "http://localhost:8080/api/v1/adapters/custom_code_adapter_abc123/upload"
# }
```

#### Upload Adapter Weights
```bash
# Upload adapter file (assuming adapter.aos file exists)
curl -X PUT \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/octet-stream" \
  --data-binary @adapter.aos \
  http://localhost:8080/api/v1/adapters/custom_code_adapter_abc123/upload
```

### Training Management

#### List Training Jobs
```bash
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/api/v1/training/jobs

# Response: Array of training jobs
# [
#   {
#     "id": "train_20251103_001",
#     "name": "Code Adapter Training",
#     "status": "running",
#     "progress": 0.67,
#     "created_at": "2025-11-03T12:00:00Z",
#     "estimated_completion": "2025-11-03T18:00:00Z"
#   }
# ]
```

#### Start New Training Job
```bash
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Python Expert Adapter",
    "base_model": "qwen2.5-7b-instruct",
    "dataset": {
      "path": "/data/training/python-code.jsonl",
      "format": "jsonl",
      "size": 100000
    },
    "config": {
      "rank": 16,
      "epochs": 3,
      "batch_size": 4,
      "learning_rate": 0.0001,
      "lora_alpha": 32
    },
    "validation": {
      "split_ratio": 0.1,
      "metrics": ["perplexity", "accuracy"]
    }
  }' \
  http://localhost:8080/api/v1/training/start

# Response:
# {
#   "id": "train_20251103_002",
#   "status": "queued",
#   "queue_position": 1,
#   "estimated_start": "2025-11-03T13:00:00Z"
# }
```

#### Get Training Job Status
```bash
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/api/v1/training/jobs/train_20251103_002

# Response:
# {
#   "id": "train_20251103_002",
#   "status": "running",
#   "progress": 0.23,
#   "current_epoch": 1,
#   "metrics": {
#     "loss": 2.145,
#     "perplexity": 8.543,
#     "learning_rate": 0.0001
#   },
#   "time_remaining": "4h 32m"
# }
```

### Inference and Chat

#### Perform Inference
```bash
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "Write a Python function to calculate fibonacci numbers",
    "adapters": ["code_lang_v1"],
    "max_tokens": 200,
    "temperature": 0.7,
    "stream": false
  }' \
  http://localhost:8080/api/v1/chat/completions

# Response:
# {
#   "id": "inf_20251103_abc123",
#   "response": "def fibonacci(n):\n    if n <= 1:\n        return n\n    return fibonacci(n-1) + fibonacci(n-2)\n\n# Example usage\nprint(fibonacci(10))  # Output: 55",
#   "usage": {
#     "prompt_tokens": 12,
#     "completion_tokens": 45,
#     "total_tokens": 57
#   },
#   "adapters_used": ["code_lang_v1"],
#   "latency_ms": 234
# }
```

#### Streaming Inference
```bash
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "Explain quantum computing in simple terms",
    "adapters": ["science_explainer"],
    "max_tokens": 500,
    "temperature": 0.8,
    "stream": true
  }' \
  http://localhost:8080/api/v1/chat/completions

# Response: Server-sent events stream
# data: {"chunk": "Quantum", "finished": false}
# data: {"chunk": " computing", "finished": false}
# data: {"chunk": " is a", "finished": false}
# ...
# data: {"finished": true, "usage": {"total_tokens": 324}}
```

### Repository Management

#### List Repositories
```bash
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/api/v1/repositories

# Response:
# [
#   {
#     "id": "github-com-user-my-project-a1b2c3d4e5f6",
#     "url": "https://github.com/user/my-project",
#     "branch": "main",
#     "path": "/var/bundles/repos/tenant-1/github-com-user-my-project-a1b2c3d4e5f6",
#     "commit_count": 128,
#     "last_scan": "2025-11-03T10:00:00Z"
#   }
# ]
```

### Metrics and Monitoring

#### Get System Metrics
```bash
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/api/v1/metrics/system

# Response:
# {
#   "memory": {
#     "used_bytes": 8589934592,
#     "total_bytes": 17179869184,
#     "headroom_pct": 22.5
#   },
#   "inference": {
#     "active_requests": 3,
#     "queue_depth": 12,
#     "avg_latency_ms": 145.2,
#     "tokens_per_sec": 234.5
#   },
#   "adapters": {
#     "loaded_count": 8,
#     "total_registered": 25,
#     "memory_usage_bytes": 2147483648
#   }
# }
```

#### Get Adapter Performance Metrics
```bash
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/api/v1/metrics/adapters/code_lang_v1

# Response:
# {
#   "activation_count": 1250,
#   "success_rate": 0.987,
#   "avg_latency_ms": 45.2,
#   "peak_memory_mb": 128,
#   "usage_patterns": {
#     "hourly": [12, 34, 28, 45, 67, 89, 123, 145, ...],
#     "daily": [890, 1200, 1456, 1345, 1678, ...]
#   }
# }
```

### Error Handling Examples

#### Invalid Request
```bash
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"invalid": "request"}' \
  http://localhost:8080/api/v1/chat/completions

# Response (400 Bad Request):
# {
#   "error": "validation_error",
#   "message": "Missing required field: prompt",
#   "details": {
#     "field": "prompt",
#     "expected": "string"
#   }
# }
```

#### Authentication Error
```bash
curl http://localhost:8080/api/v1/adapters

# Response (401 Unauthorized):
# {
#   "error": "unauthorized",
#   "message": "Missing or invalid authentication token"
# }
```

### Real-World Integration Examples

#### Python Client
```python
import requests
import json

class AdapterOSClient:
    def __init__(self, base_url="http://localhost:8080/api"):
        self.base_url = base_url
        self.token = None

    def login(self, email, password):
        response = requests.post(
            f"{self.base_url}/v1/auth/login",
            json={"email": email, "password": password}
        )
        self.token = response.json()["token"]
        return self.token

    def chat(self, prompt, adapters=None, **kwargs):
        headers = {"Authorization": f"Bearer {self.token}"}
        data = {
            "prompt": prompt,
            "adapters": adapters or [],
            **kwargs
        }
        response = requests.post(
            f"{self.base_url}/v1/chat/completions",
            headers=headers,
            json=data
        )
        return response.json()

# Usage
client = AdapterOSClient()
client.login("admin@example.com", "password")
result = client.chat(
    "Write a factorial function in Python",
    adapters=["code_lang_v1"],
    max_tokens=100
)
print(result["response"])
```

#### JavaScript/Node.js Client
```javascript
class AdapterOSAPI {
  constructor(baseURL = 'http://localhost:8080/api') {
    this.baseURL = baseURL;
    this.token = null;
  }

  async login(email, password) {
    const response = await fetch(`${this.baseURL}/v1/auth/login`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email, password })
    });
    const data = await response.json();
    this.token = data.token;
    return data;
  }

  async chat(prompt, options = {}) {
    const response = await fetch(`${this.baseURL}/v1/chat/completions`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${this.token}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        prompt,
        adapters: options.adapters || [],
        max_tokens: options.maxTokens || 200,
        temperature: options.temperature || 0.7,
        stream: options.stream || false
      })
    });
    return response.json();
  }
}

// Usage
const api = new AdapterOSAPI();
await api.login('admin@example.com', 'password');
const result = await api.chat('Explain recursion', {
  adapters: ['teaching_assistant'],
  maxTokens: 300
});
console.log(result.response);
```

#### Go Client
```go
package main

import (
    "bytes"
    "encoding/json"
    "fmt"
    "io"
    "net/http"
)

type AdapterOSClient struct {
    BaseURL string
    Token   string
}

type LoginRequest struct {
    Email    string `json:"email"`
    Password string `json:"password"`
}

type ChatRequest struct {
    Prompt    string   `json:"prompt"`
    Adapters  []string `json:"adapters,omitempty"`
    MaxTokens int      `json:"max_tokens,omitempty"`
    Temperature float64 `json:"temperature,omitempty"`
    Stream    bool     `json:"stream,omitempty"`
}

func (c *AdapterOSClient) Login(email, password string) error {
    req := LoginRequest{Email: email, Password: password}
    jsonData, _ := json.Marshal(req)

    resp, err := http.Post(c.BaseURL+"/v1/auth/login", "application/json", bytes.NewBuffer(jsonData))
    if err != nil {
        return err
    }
    defer resp.Body.Close()

    var result map[string]interface{}
    json.NewDecoder(resp.Body).Decode(&result)
    c.Token = result["token"].(string)
    return nil
}

func (c *AdapterOSClient) Chat(prompt string, adapters []string) (map[string]interface{}, error) {
    req := ChatRequest{
        Prompt: prompt,
        Adapters: adapters,
        MaxTokens: 200,
    }
    jsonData, _ := json.Marshal(req)

    httpReq, _ := http.NewRequest("POST", c.BaseURL+"/v1/chat/completions", bytes.NewBuffer(jsonData))
    httpReq.Header.Set("Authorization", "Bearer "+c.Token)
    httpReq.Header.Set("Content-Type", "application/json")

    client := &http.Client{}
    resp, err := client.Do(httpReq)
    if err != nil {
        return nil, err
    }
    defer resp.Body.Close()

    var result map[string]interface{}
    json.NewDecoder(resp.Body).Decode(&result)
    return result, nil
}

func main() {
    client := &AdapterOSClient{BaseURL: "http://localhost:8080/api"}
    client.Login("admin@example.com", "password")

    result, _ := client.Chat("Hello, AdapterOS!", []string{"general_assistant"})
    fmt.Println(result["response"])
}
```
=======
      },
      "QualityMetricsResponse": {
        "type": "object",
        "description": "Quality metrics response",
        "required": [
          "arr",
          "ecs5",
          "hlr",
          "cr",
          "timestamp"
        ],
        "properties": {
          "arr": {
            "type": "number",
            "format": "float"
          },
          "cr": {
            "type": "number",
            "format": "float"
          },
          "ecs5": {
            "type": "number",
            "format": "float"
          },
          "hlr": {
            "type": "number",
            "format": "float"
          },
          "timestamp": {
            "type": "string"
          }
        }
      },
      "RegisterAdapterRequest": {
        "type": "object",
        "description": "Register adapter request",
        "required": [
          "adapter_id",
          "name",
          "hash_b3",
          "rank",
          "tier",
          "languages"
        ],
        "properties": {
          "adapter_id": {
            "type": "string"
          },
          "framework": {
            "type": "string",
            "nullable": true
          },
          "hash_b3": {
            "type": "string"
          },
          "languages": {
            "type": "array",
            "items": {
              "type": "string"
            }
          },
          "name": {
            "type": "string"
          },
          "rank": {
            "type": "integer",
            "format": "int32"
          },
          "tier": {
            "type": "integer",
            "format": "int32"
          }
        }
      },
      "RegisterRepositoryRequest": {
        "type": "object",
        "description": "Register repository request",
        "required": [
          "repo_id",
          "path",
          "languages",
          "default_branch"
        ],
        "properties": {
          "default_branch": {
            "type": "string"
          },
          "languages": {
            "type": "array",
            "items": {
              "type": "string"
            }
          },
          "path": {
            "type": "string"
          },
          "repo_id": {
            "type": "string"
          }
        }
      },
      "RepositoryResponse": {
        "type": "object",
        "description": "Repository response",
        "required": [
          "id",
          "repo_id",
          "path",
          "languages",
          "default_branch",
          "status",
          "frameworks",
          "created_at",
          "updated_at"
        ],
        "properties": {
          "created_at": {
            "type": "string"
          },
          "default_branch": {
            "type": "string"
          },
          "file_count": {
            "type": "integer",
            "format": "int64",
            "nullable": true
          },
          "frameworks": {
            "type": "array",
            "items": {
              "type": "string"
            }
          },
          "id": {
            "type": "string"
          },
          "languages": {
            "type": "array",
            "items": {
              "type": "string"
            }
          },
          "path": {
            "type": "string"
          },
          "repo_id": {
            "type": "string"
          },
          "status": {
            "type": "string"
          },
          "symbol_count": {
            "type": "integer",
            "format": "int64",
            "nullable": true
          },
          "updated_at": {
            "type": "string"
          }
        }
      },
      "RouterDecision": {
        "type": "object",
        "description": "Canonical router decision per token (frozen schema)",
        "required": [
          "step",
          "candidate_adapters",
          "entropy",
          "tau",
          "entropy_floor"
        ],
        "properties": {
          "step": {
            "type": "integer",
            "minimum": 0
          },
          "input_token_id": {
            "type": "integer",
            "format": "int32",
            "minimum": 0,
            "nullable": true
          },
          "candidate_adapters": {
            "type": "array",
            "items": {
              "$ref": "#/components/schemas/RouterCandidate"
            }
          },
          "entropy": {
            "type": "number"
          },
          "tau": {
            "type": "number"
          },
          "entropy_floor": {
            "type": "number"
          },
          "stack_hash": {
            "type": "string",
            "nullable": true
          }
        }
      },
      "RouterCandidate": {
        "type": "object",
        "description": "Candidate adapter entry (ordered by raw_score)",
        "required": [
          "adapter_idx",
          "raw_score",
          "gate_q15"
        ],
        "properties": {
          "adapter_idx": {
            "type": "integer",
            "minimum": 0
          },
          "raw_score": {
            "type": "number"
          },
          "gate_q15": {
            "type": "integer"
          }
        }
      },
      "RoutingDebugRequest": {
        "type": "object",
        "description": "Routing debug request",
        "required": [
          "prompt"
        ],
        "properties": {
          "context": {
            "type": "string",
            "nullable": true
          },
          "prompt": {
            "type": "string"
          }
        }
      },
      "RoutingDebugResponse": {
        "type": "object",
        "description": "Routing debug response",
        "required": [
          "features",
          "adapter_scores",
          "selected_adapters",
          "explanation"
        ],
        "properties": {
          "adapter_scores": {
            "type": "array",
            "items": {
              "$ref": "#/components/schemas/AdapterScore"
            }
          },
          "explanation": {
            "type": "string"
          },
          "features": {
            "$ref": "#/components/schemas/FeatureVector"
          },
          "selected_adapters": {
            "type": "array",
            "items": {
              "type": "string"
            }
          }
        }
      },
      "RoutingDecision": {
        "type": "object",
        "description": "Single routing decision",
        "required": [
          "ts",
          "tenant_id",
          "adapters_used",
          "activations",
          "reason",
          "trace_id"
        ],
        "properties": {
          "activations": {
            "type": "array",
            "items": {
              "type": "number",
              "format": "double"
            }
          },
          "adapters_used": {
            "type": "array",
            "items": {
              "type": "string"
            }
          },
          "reason": {
            "type": "string"
          },
          "tenant_id": {
            "type": "string"
          },
          "trace_id": {
            "type": "string"
          },
          "ts": {
            "type": "string"
          }
        }
      },
      "RoutingDecisionsQuery": {
        "type": "object",
        "description": "Routing decisions query parameters",
        "required": [
          "tenant"
        ],
        "properties": {
          "limit": {
            "type": "integer",
            "minimum": 0
          },
          "since": {
            "type": "string",
            "nullable": true
          },
          "tenant": {
            "type": "string"
          }
        }
      },
      "RoutingDecisionsResponse": {
        "type": "object",
        "description": "Routing decisions response",
        "required": [
          "items"
        ],
        "properties": {
          "items": {
            "type": "array",
            "items": {
              "$ref": "#/components/schemas/RoutingDecision"
            }
          }
        }
      },
      "ScanStatusResponse": {
        "type": "object",
        "description": "Scan status response",
        "required": [
          "repo_id",
          "status"
        ],
        "properties": {
          "message": {
            "type": "string",
            "nullable": true
          },
          "progress": {
            "type": "number",
            "format": "float",
            "nullable": true
          },
          "repo_id": {
            "type": "string"
          },
          "status": {
            "type": "string"
          }
        }
      },
      "SessionAction": {
        "type": "string",
        "description": "Session action",
        "enum": [
          "merge",
          "abandon"
        ]
      },
      "StartGitSessionRequest": {
        "type": "object",
        "description": "Start Git session request",
        "required": [
          "adapter_id",
          "repo_id"
        ],
        "properties": {
          "adapter_id": {
            "type": "string"
          },
          "base_branch": {
            "type": "string",
            "nullable": true
          },
          "repo_id": {
            "type": "string"
          }
        }
      },
      "StartGitSessionResponse": {
        "type": "object",
        "description": "Start Git session response",
        "required": [
          "session_id",
          "branch_name"
        ],
        "properties": {
          "branch_name": {
            "type": "string"
          },
          "session_id": {
            "type": "string"
          }
        }
      },
      "StartTrainingRequest": {
        "type": "object",
        "description": "Start training request",
        "required": [
          "adapter_name",
          "config"
        ],
        "properties": {
          "adapter_name": {
            "type": "string"
          },
          "config": {
            "$ref": "#/components/schemas/TrainingConfigRequest"
          },
          "repo_id": {
            "type": "string",
            "nullable": true
          },
          "template_id": {
            "type": "string",
            "nullable": true
          }
        }
      },
      "StreamQuery": {
        "type": "object",
        "description": "Stream query parameters (for training and contacts streams)",
        "required": [
          "tenant"
        ],
        "properties": {
          "tenant": {
            "type": "string"
          }
        }
      },
      "SystemMetricsResponse": {
        "type": "object",
        "description": "System metrics response",
        "required": [
          "cpu_usage",
          "memory_usage",
          "active_workers",
          "requests_per_second",
          "avg_latency_ms",
          "disk_usage",
          "network_bandwidth",
          "gpu_utilization",
          "uptime_seconds",
          "process_count",
          "load_average",
          "timestamp"
        ],
        "properties": {
          "active_workers": {
            "type": "integer",
            "format": "int32"
          },
          "avg_latency_ms": {
            "type": "number",
            "format": "float"
          },
          "cpu_usage": {
            "type": "number",
            "format": "float"
          },
          "disk_usage": {
            "type": "number",
            "format": "float"
          },
          "gpu_utilization": {
            "type": "number",
            "format": "float"
          },
          "load_average": {
            "$ref": "#/components/schemas/LoadAverageResponse"
          },
          "memory_usage": {
            "type": "number",
            "format": "float"
          },
          "network_bandwidth": {
            "type": "number",
            "format": "float"
          },
          "process_count": {
            "type": "integer",
            "minimum": 0
          },
          "requests_per_second": {
            "type": "number",
            "format": "float"
          },
          "timestamp": {
            "type": "integer",
            "format": "int64",
            "minimum": 0
          },
          "uptime_seconds": {
            "type": "integer",
            "format": "int64",
            "minimum": 0
          }
        }
      },
      "TenantResponse": {
        "type": "object",
        "description": "Tenant response",
        "required": [
          "id",
          "name",
          "itar_flag",
          "created_at"
        ],
        "properties": {
          "created_at": {
            "type": "string"
          },
          "id": {
            "type": "string"
          },
          "itar_flag": {
            "type": "boolean"
          },
          "name": {
            "type": "string"
          }
        }
      },
      "TestDomainAdapterRequest": {
        "type": "object",
        "description": "Test domain adapter request",
        "required": [
          "adapter_id",
          "input_data"
        ],
        "properties": {
          "adapter_id": {
            "type": "string"
          },
          "expected_output": {
            "type": "string",
            "nullable": true
          },
          "input_data": {
            "type": "string"
          },
          "iterations": {
            "type": "integer",
            "format": "int32",
            "nullable": true,
            "minimum": 0
          }
        }
      },
      "TestDomainAdapterResponse": {
        "type": "object",
        "description": "Test domain adapter response",
        "required": [
          "test_id",
          "adapter_id",
          "input_data",
          "actual_output",
          "passed",
          "iterations",
          "execution_time_ms",
          "executed_at"
        ],
        "properties": {
          "actual_output": {
            "type": "string"
          },
          "adapter_id": {
            "type": "string"
          },
          "epsilon": {
            "type": "number",
            "format": "double",
            "nullable": true
          },
          "executed_at": {
            "type": "string"
          },
          "execution_time_ms": {
            "type": "integer",
            "format": "int64",
            "minimum": 0
          },
          "expected_output": {
            "type": "string",
            "nullable": true
          },
          "input_data": {
            "type": "string"
          },
          "iterations": {
            "type": "integer",
            "format": "int32",
            "minimum": 0
          },
          "passed": {
            "type": "boolean"
          },
          "test_id": {
            "type": "string"
          }
        }
      },
      "TrainingConfigRequest": {
        "type": "object",
        "description": "Training configuration request",
        "required": [
          "rank",
          "alpha",
          "targets",
          "epochs",
          "learning_rate",
          "batch_size"
        ],
        "properties": {
          "alpha": {
            "type": "integer",
            "format": "int32",
            "minimum": 0
          },
          "batch_size": {
            "type": "integer",
            "format": "int32",
            "minimum": 0
          },
          "epochs": {
            "type": "integer",
            "format": "int32",
            "minimum": 0
          },
          "gradient_accumulation_steps": {
            "type": "integer",
            "format": "int32",
            "nullable": true,
            "minimum": 0
          },
          "learning_rate": {
            "type": "number",
            "format": "float"
          },
          "max_seq_length": {
            "type": "integer",
            "format": "int32",
            "nullable": true,
            "minimum": 0
          },
          "rank": {
            "type": "integer",
            "format": "int32",
            "minimum": 0
          },
          "targets": {
            "type": "array",
            "items": {
              "type": "string"
            }
          },
          "warmup_steps": {
            "type": "integer",
            "format": "int32",
            "nullable": true,
            "minimum": 0
          }
        }
      },
      "TrainingJobResponse": {
        "type": "object",
        "description": "Training job response",
        "required": [
          "id",
          "adapter_name",
          "status",
          "progress_pct",
          "current_epoch",
          "total_epochs",
          "current_loss",
          "learning_rate",
          "tokens_per_second",
          "created_at"
        ],
        "properties": {
          "adapter_name": {
            "type": "string"
          },
          "completed_at": {
            "type": "string",
            "nullable": true
          },
          "created_at": {
            "type": "string"
          },
          "current_epoch": {
            "type": "integer",
            "format": "int32",
            "minimum": 0
          },
          "current_loss": {
            "type": "number",
            "format": "float"
          },
          "error_message": {
            "type": "string",
            "nullable": true
          },
          "id": {
            "type": "string"
          },
          "learning_rate": {
            "type": "number",
            "format": "float"
          },
          "progress_pct": {
            "type": "number",
            "format": "float"
          },
          "repo_id": {
            "type": "string",
            "nullable": true
          },
          "started_at": {
            "type": "string",
            "nullable": true
          },
          "status": {
            "type": "string"
          },
          "template_id": {
            "type": "string",
            "nullable": true
          },
          "tokens_per_second": {
            "type": "number",
            "format": "float"
          },
          "total_epochs": {
            "type": "integer",
            "format": "int32",
            "minimum": 0
          }
        }
      },
      "TrainingMetricsResponse": {
        "type": "object",
        "description": "Training metrics response",
        "required": [
          "loss",
          "tokens_per_second",
          "learning_rate",
          "current_epoch",
          "total_epochs",
          "progress_pct"
        ],
        "properties": {
          "current_epoch": {
            "type": "integer",
            "format": "int32",
            "minimum": 0
          },
          "learning_rate": {
            "type": "number",
            "format": "float"
          },
          "loss": {
            "type": "number",
            "format": "float"
          },
          "progress_pct": {
            "type": "number",
            "format": "float"
          },
          "tokens_per_second": {
            "type": "number",
            "format": "float"
          },
          "total_epochs": {
            "type": "integer",
            "format": "int32",
            "minimum": 0
          }
        }
      },
      "TrainingTemplateResponse": {
        "type": "object",
        "description": "Training template response",
        "required": [
          "id",
          "name",
          "description",
          "category",
          "rank",
          "alpha",
          "targets",
          "epochs",
          "learning_rate",
          "batch_size"
        ],
        "properties": {
          "alpha": {
            "type": "integer",
            "format": "int32",
            "minimum": 0
          },
          "batch_size": {
            "type": "integer",
            "format": "int32",
            "minimum": 0
          },
          "category": {
            "type": "string"
          },
          "description": {
            "type": "string"
          },
          "epochs": {
            "type": "integer",
            "format": "int32",
            "minimum": 0
          },
          "id": {
            "type": "string"
          },
          "learning_rate": {
            "type": "number",
            "format": "float"
          },
          "name": {
            "type": "string"
          },
          "rank": {
            "type": "integer",
            "format": "int32",
            "minimum": 0
          },
          "targets": {
            "type": "array",
            "items": {
              "type": "string"
            }
          }
        }
      },
      "TriggerScanRequest": {
        "type": "object",
        "description": "Trigger scan request",
        "required": [
          "repo_id"
        ],
        "properties": {
          "repo_id": {
            "type": "string"
          }
        }
      }
    }
  },
  "tags": [
    {
      "name": "health",
      "description": "Health check endpoints"
    },
    {
      "name": "auth",
      "description": "Authentication endpoints"
    },
    {
      "name": "tenants",
      "description": "Tenant management"
    },
    {
      "name": "nodes",
      "description": "Node management"
    },
    {
      "name": "models",
      "description": "Model registry"
    },
    {
      "name": "jobs",
      "description": "Job management"
    },
    {
      "name": "code",
      "description": "Code intelligence operations"
    },
    {
      "name": "adapters",
      "description": "Adapter management"
    },
    {
      "name": "repositories",
      "description": "Repository management"
    },
    {
      "name": "metrics",
      "description": "System and quality metrics"
    },
    {
      "name": "commits",
      "description": "Commit inspection"
    },
    {
      "name": "routing",
      "description": "Routing debug and inspection"
    },
    {
      "name": "contacts",
      "description": "Contact discovery and management"
    },
    {
      "name": "streams",
      "description": "Real-time SSE event streams"
    },
    {
      "name": "domain-adapters",
      "description": "Domain adapter management"
    },
    {
      "name": "git",
      "description": "Git integration and session management"
    }
  ]
}
```

## API Endpoints Summary

The API provides comprehensive endpoints for:

- **Authentication** - Login and JWT token management
- **Adapters** - Register, list, and manage adapters
- **Repositories** - Git repository management and scanning
- **Training** - Training job management and monitoring
- **Domain Adapters** - Domain-specific adapter execution
- **Metrics** - System and adapter performance metrics
- **Contacts** - Contact discovery and management
- **Streams** - Real-time SSE event streams
- **Health** - Health and readiness checks
>>>>>>> integration-branch

## Development

To interact with the API:

1. **Swagger UI:** http://localhost:8080/api/swagger-ui/
2. **OpenAPI Spec:** http://localhost:8080/api/api-docs/openapi.json
3. **API Base URL:** http://localhost:8080/api/

## Authentication

All protected endpoints require a JWT token obtained from the login endpoint:

```bash
# Login
curl -X POST http://localhost:8080/api/v1/auth/login \
  -H "Content-Type: application/json" \
<<<<<<< HEAD
  -d '{"email":"admin@example.com","password":"password"}'
=======
  -d '{"email":"admin@aos.local","password":"password"}'
>>>>>>> integration-branch

# Use token in subsequent requests
curl -H "Authorization: Bearer <token>" \
  http://localhost:8080/api/v1/adapters
```

<<<<<<< HEAD
## API Endpoints Summary

The API provides comprehensive endpoints for:

- **Authentication** - Login and JWT token management
- **Adapters** - Register, list, and manage adapters
- **Models** - Import and manage base models
- **Training** - Training job management and monitoring
- **Inference** - Chat completions with adapter selection
- **Metrics** - System and adapter performance metrics
- **Repositories** - Git repository management and scanning
- **Health** - Health and readiness checks

Generated on 2025-01-15
=======
>>>>>>> integration-branch
Generated on Tue Oct 14 08:33:47 CDT 2025
