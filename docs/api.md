# AdapterOS Server API Documentation

This document contains the complete OpenAPI specification for the AdapterOS Server API.

## Overview

The AdapterOS Server API provides endpoints for managing tenants, adapters, repositories, training jobs, and more in the AdapterOS system.

## Demo Credentials

The following demo credentials are available for testing:

- **Admin:** admin@example.com / password

## OpenAPI Specification

```json
{
  "openapi": "3.0.3",
  "info": {
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
        "tags": ["health"],
        "summary": "Readiness check",
        "operationId": "readiness_check",
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
          }
        }
      }
    },
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
                }
              }
            }
          }
        }
      }
    },
    "/v1/auth/login": {
      "post": {
        "tags": ["authentication"],
        "summary": "Login with credentials",
        "operationId": "auth_login",
        "requestBody": {
          "required": true,
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/LoginRequest"
              }
            }
          }
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
            "content": {
              "application/json": {
                "schema": {
                  "type": "array",
                  "items": {
                    "$ref": "#/components/schemas/ModelInfo"
                  }
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
          }
        ],
        "responses": {
          "200": {
            "description": "Model details",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ModelDetails"
                }
              }
            }
          },
          "404": {
            "description": "Model not found",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
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
            "content": {
              "application/json": {
                "schema": {
                  "type": "array",
                  "items": {
                    "$ref": "#/components/schemas/AdapterInfo"
                  }
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
    "/v1/adapters/{adapter_id}": {
      "get": {
        "tags": ["adapters"],
        "summary": "Get adapter details",
        "operationId": "get_adapter",
        "parameters": [
          {
            "name": "adapter_id",
            "in": "path",
            "required": true,
            "schema": {
              "type": "string"
            },
            "description": "Adapter ID"
          }
        ],
        "responses": {
          "200": {
            "description": "Adapter details",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/AdapterDetails"
                }
              }
            }
          },
          "404": {
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
        "parameters": [
          {
            "name": "adapter_id",
            "in": "path",
            "required": true,
            "schema": {
              "type": "string"
            },
            "description": "Adapter ID"
          }
        ],
        "responses": {
          "204": {
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
    "/v1/domain-adapters": {
      "get": {
        "tags": ["domain-adapters"],
        "summary": "List all domain adapters",
        "operationId": "list_domain_adapters",
        "responses": {
          "200": {
            "description": "List of domain adapters",
            "content": {
              "application/json": {
                "schema": {
                  "type": "array",
                  "items": {
                    "$ref": "#/components/schemas/DomainAdapterResponse"
                  }
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
      },
      "post": {
        "tags": ["domain-adapters"],
        "summary": "Create a new domain adapter",
        "operationId": "create_domain_adapter",
        "requestBody": {
          "required": true,
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/CreateDomainAdapterRequest"
              }
            }
          }
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
          }
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
      }
    },
    "/v1/domain-adapters/{adapter_id}": {
      "get": {
        "tags": ["domain-adapters"],
        "summary": "Get domain adapter details",
        "operationId": "get_domain_adapter",
        "parameters": [
          {
            "name": "adapter_id",
            "in": "path",
            "required": true,
            "schema": {
              "type": "string"
            },
            "description": "Domain adapter ID"
          }
        ],
        "responses": {
          "200": {
            "description": "Domain adapter details",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/DomainAdapterResponse"
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
      },
      "delete": {
        "tags": ["domain-adapters"],
        "summary": "Delete domain adapter",
        "operationId": "delete_domain_adapter",
        "parameters": [
          {
            "name": "adapter_id",
            "in": "path",
            "required": true,
            "schema": {
              "type": "string"
            },
            "description": "Domain adapter ID"
          }
        ],
        "responses": {
          "204": {
            "description": "Domain adapter deleted"
          }
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
      }
    },
    "/v1/domain-adapters/{adapter_id}/load": {
      "post": {
        "tags": ["domain-adapters"],
        "summary": "Load domain adapter into deterministic executor",
        "operationId": "load_domain_adapter",
        "parameters": [
          {
            "name": "adapter_id",
            "in": "path",
            "required": true,
            "schema": {
              "type": "string"
            },
            "description": "Domain adapter ID"
          }
        ],
        "requestBody": {
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/LoadDomainAdapterRequest"
              }
            }
          }
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
          }
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
      }
    },
    "/v1/domain-adapters/{adapter_id}/unload": {
      "post": {
        "tags": ["domain-adapters"],
        "summary": "Unload domain adapter from memory",
        "operationId": "unload_domain_adapter",
        "parameters": [
          {
            "name": "adapter_id",
            "in": "path",
            "required": true,
            "schema": {
              "type": "string"
            },
            "description": "Domain adapter ID"
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
          }
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
      }
    },
    "/v1/domain-adapters/{adapter_id}/execute": {
      "post": {
        "tags": ["domain-adapters"],
        "summary": "Execute domain adapter on input data",
        "operationId": "execute_domain_adapter",
        "parameters": [
          {
            "name": "adapter_id",
            "in": "path",
            "required": true,
            "schema": {
              "type": "string"
            },
            "description": "Domain adapter ID"
          }
        ],
        "requestBody": {
          "required": true,
          "content": {
            "application/json": {
              "schema": {
                "type": "object",
                "description": "Domain-specific input data (varies by adapter type)"
              }
            }
          }
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
          }
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
      }
    },
    "/v1/domain-adapters/{adapter_id}/test": {
      "post": {
        "tags": ["domain-adapters"],
        "summary": "Run determinism testing on domain adapter",
        "operationId": "test_domain_adapter",
        "parameters": [
          {
            "name": "adapter_id",
            "in": "path",
            "required": true,
            "schema": {
              "type": "string"
            },
            "description": "Domain adapter ID"
          }
        ],
        "requestBody": {
          "required": true,
          "content": {
            "application/json": {
              "schema": {
                "$ref": "#/components/schemas/TestDomainAdapterRequest"
              }
            }
          }
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
          }
        },
        "security": [
          {
            "bearer_token": []
          }
        ]
      }
    },
    "/v1/domain-adapters/{adapter_id}/manifest": {
      "get": {
        "tags": ["domain-adapters"],
        "summary": "Get domain adapter manifest",
        "operationId": "get_domain_adapter_manifest",
        "parameters": [
          {
            "name": "adapter_id",
            "in": "path",
            "required": true,
            "schema": {
              "type": "string"
            },
            "description": "Domain adapter ID"
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
            "content": {
              "application/json": {
                "schema": {
                  "type": "array",
                  "items": {
                    "$ref": "#/components/schemas/TrainingJobInfo"
                  }
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
                }
              }
            }
          },
          "404": {
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
    "/v1/metrics/system": {
      "get": {
        "tags": ["metrics"],
        "summary": "Get system metrics",
        "operationId": "get_system_metrics",
        "responses": {
          "200": {
            "description": "System metrics",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/SystemMetrics"
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
    "/v1/metrics/system/history": {
      "get": {
        "tags": ["metrics"],
        "summary": "Get historical system metrics",
        "operationId": "get_system_metrics_history",
        "parameters": [
          {
            "name": "hours",
            "in": "query",
            "schema": {
              "type": "integer",
              "default": 24,
              "minimum": 1,
              "maximum": 720
            },
            "description": "Number of hours of historical data to retrieve"
          },
          {
            "name": "limit",
            "in": "query",
            "schema": {
              "type": "integer",
              "default": 1000,
              "minimum": 1,
              "maximum": 10000
            },
            "description": "Maximum number of records to return"
          }
        ],
        "responses": {
          "200": {
            "description": "Historical system metrics",
            "content": {
              "application/json": {
                "schema": {
                  "type": "array",
                  "items": {
                    "$ref": "#/components/schemas/SystemMetricsRecord"
                  }
                }
              }
            }
          },
          "400": {
            "description": "Invalid parameters",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/ErrorResponse"
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
          }
        ],
        "responses": {
          "200": {
            "description": "Adapter metrics",
            "content": {
              "application/json": {
                "schema": {
                  "$ref": "#/components/schemas/AdapterMetrics"
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
    "/v1/repositories": {
      "get": {
        "tags": ["repositories"],
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
                    "$ref": "#/components/schemas/RepositoryInfo"
                  }
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
    }
  },
  "components": {
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
          "code": {
            "type": "string"
          },
          "details": {
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
            "type": "string"
          }
        }
      },
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
          "adapters_used": {
            "type": "array",
            "items": {
              "type": "string"
            }
          },
          "latency_ms": {
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
      "SystemMetricsRecord": {
        "type": "object",
        "properties": {
          "id": {
            "type": "integer",
            "format": "int64",
            "description": "Unique record identifier"
          },
          "timestamp": {
            "type": "integer",
            "format": "int64",
            "description": "Unix timestamp in seconds"
          },
          "cpu_usage": {
            "type": "number",
            "format": "float",
            "description": "CPU usage percentage (0-100)"
          },
          "memory_usage": {
            "type": "number",
            "format": "float",
            "description": "Memory usage percentage (0-100)"
          },
          "disk_read_bytes": {
            "type": "integer",
            "format": "int64",
            "description": "Bytes read from disk"
          },
          "disk_write_bytes": {
            "type": "integer",
            "format": "int64",
            "description": "Bytes written to disk"
          },
          "disk_usage_percent": {
            "type": "number",
            "format": "float",
            "description": "Disk usage percentage"
          },
          "network_rx_bytes": {
            "type": "integer",
            "format": "int64",
            "description": "Network bytes received"
          },
          "network_tx_bytes": {
            "type": "integer",
            "format": "int64",
            "description": "Network bytes transmitted"
          },
          "network_rx_packets": {
            "type": "integer",
            "format": "int64",
            "description": "Network packets received"
          },
          "network_tx_packets": {
            "type": "integer",
            "format": "int64",
            "description": "Network packets transmitted"
          },
          "network_bandwidth_mbps": {
            "type": "number",
            "format": "float",
            "description": "Network bandwidth in Mbps"
          },
          "gpu_utilization": {
            "type": "number",
            "format": "float",
            "nullable": true,
            "description": "GPU utilization percentage"
          },
          "gpu_memory_used": {
            "type": "integer",
            "format": "int64",
            "nullable": true,
            "description": "GPU memory used in bytes"
          },
          "gpu_memory_total": {
            "type": "integer",
            "format": "int64",
            "nullable": true,
            "description": "Total GPU memory in bytes"
          },
          "uptime_seconds": {
            "type": "integer",
            "format": "int64",
            "description": "System uptime in seconds"
          },
          "process_count": {
            "type": "integer",
            "format": "int32",
            "description": "Number of running processes"
          },
          "load_1min": {
            "type": "number",
            "format": "float",
            "description": "1-minute load average"
          },
          "load_5min": {
            "type": "number",
            "format": "float",
            "description": "5-minute load average"
          },
          "load_15min": {
            "type": "number",
            "format": "float",
            "description": "15-minute load average"
          }
        },
        "required": [
          "id",
          "timestamp",
          "cpu_usage",
          "memory_usage",
          "disk_read_bytes",
          "disk_write_bytes",
          "disk_usage_percent",
          "network_rx_bytes",
          "network_tx_bytes",
          "network_rx_packets",
          "network_tx_packets",
          "network_bandwidth_mbps",
          "uptime_seconds",
          "process_count",
          "load_1min",
          "load_5min",
          "load_15min"
        ]
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
          },
          "status": {
            "type": "string"
          }
        }
      },
      "DomainAdapterResponse": {
        "type": "object",
        "properties": {
          "id": {
            "type": "string",
            "description": "Unique domain adapter identifier"
          },
          "name": {
            "type": "string",
            "description": "Human-readable adapter name"
          },
          "version": {
            "type": "string",
            "description": "Adapter version"
          },
          "description": {
            "type": "string",
            "description": "Adapter description"
          },
          "domain_type": {
            "type": "string",
            "enum": ["code", "vision", "text", "audio", "multimodal"],
            "description": "Domain type this adapter processes"
          },
          "model": {
            "type": "string",
            "description": "Model identifier"
          },
          "status": {
            "type": "string",
            "enum": ["unloaded", "loaded", "error"],
            "description": "Current adapter status"
          },
          "execution_count": {
            "type": "integer",
            "format": "int64",
            "description": "Total execution count"
          },
          "last_execution": {
            "type": "string",
            "format": "date-time",
            "nullable": true,
            "description": "Last execution timestamp"
          },
          "created_at": {
            "type": "string",
            "format": "date-time",
            "description": "Creation timestamp"
          },
          "updated_at": {
            "type": "string",
            "format": "date-time",
            "description": "Last update timestamp"
          }
        },
        "required": ["id", "name", "version", "domain_type", "model", "status", "created_at", "updated_at"]
      },
      "CreateDomainAdapterRequest": {
        "type": "object",
        "properties": {
          "name": {
            "type": "string",
            "description": "Human-readable adapter name"
          },
          "version": {
            "type": "string",
            "description": "Adapter version"
          },
          "description": {
            "type": "string",
            "description": "Adapter description"
          },
          "domain_type": {
            "type": "string",
            "enum": ["code", "vision", "text", "audio", "multimodal"],
            "description": "Domain type this adapter processes"
          },
          "model": {
            "type": "string",
            "description": "Model identifier"
          },
          "input_format": {
            "type": "string",
            "description": "Expected input format"
          },
          "output_format": {
            "type": "string",
            "description": "Output format"
          },
          "config": {
            "type": "object",
            "description": "Domain-specific configuration"
          }
        },
        "required": ["name", "domain_type", "model"]
      },
      "LoadDomainAdapterRequest": {
        "type": "object",
        "properties": {
          "config": {
            "type": "object",
            "description": "Load-time configuration"
          }
        }
      },
      "DomainAdapterExecutionResponse": {
        "type": "object",
        "properties": {
          "execution_id": {
            "type": "string",
            "description": "Unique execution identifier"
          },
          "adapter_id": {
            "type": "string",
            "description": "Adapter that performed execution"
          },
          "input_hash": {
            "type": "string",
            "description": "BLAKE3 hash of input data"
          },
          "output_hash": {
            "type": "string",
            "description": "BLAKE3 hash of output data"
          },
          "epsilon": {
            "type": "number",
            "format": "float",
            "description": "Numerical precision drift"
          },
          "execution_time_ms": {
            "type": "integer",
            "format": "int64",
            "description": "Execution time in milliseconds"
          },
          "trace_events": {
            "type": "array",
            "items": {
              "type": "string"
            },
            "description": "Execution trace events"
          },
          "executed_at": {
            "type": "string",
            "format": "date-time",
            "description": "Execution timestamp"
          }
        },
        "required": ["execution_id", "adapter_id", "input_hash", "output_hash", "epsilon", "execution_time_ms", "trace_events", "executed_at"]
      },
      "TestDomainAdapterRequest": {
        "type": "object",
        "properties": {
          "input_data": {
            "type": "string",
            "description": "JSON input data as string"
          },
          "iterations": {
            "type": "integer",
            "default": 100,
            "description": "Number of test iterations"
          },
          "expected_output": {
            "type": "string",
            "nullable": true,
            "description": "Expected output for comparison"
          }
        },
        "required": ["input_data"]
      },
      "TestDomainAdapterResponse": {
        "type": "object",
        "properties": {
          "test_id": {
            "type": "string",
            "description": "Unique test identifier"
          },
          "adapter_id": {
            "type": "string",
            "description": "Adapter that was tested"
          },
          "input_data": {
            "type": "string",
            "description": "Input data used for testing"
          },
          "actual_output": {
            "type": "string",
            "description": "Actual output from final execution"
          },
          "expected_output": {
            "type": "string",
            "nullable": true,
            "description": "Expected output for comparison"
          },
          "epsilon": {
            "type": "number",
            "format": "float",
            "nullable": true,
            "description": "Maximum numerical drift detected"
          },
          "passed": {
            "type": "boolean",
            "description": "Whether determinism test passed (95%+ score)"
          },
          "iterations": {
            "type": "integer",
            "format": "int32",
            "description": "Number of test iterations performed"
          },
          "execution_time_ms": {
            "type": "integer",
            "format": "int64",
            "description": "Total test execution time"
          },
          "executed_at": {
            "type": "string",
            "format": "date-time",
            "description": "Test completion timestamp"
          }
        },
        "required": ["test_id", "adapter_id", "input_data", "actual_output", "passed", "iterations", "execution_time_ms", "executed_at"]
      },
      "DomainAdapterManifestResponse": {
        "type": "object",
        "properties": {
          "adapter_id": {
            "type": "string",
            "description": "Adapter identifier"
          },
          "name": {
            "type": "string",
            "description": "Adapter name"
          },
          "version": {
            "type": "string",
            "description": "Adapter version"
          },
          "description": {
            "type": "string",
            "description": "Adapter description"
          },
          "domain_type": {
            "type": "string",
            "description": "Domain type"
          },
          "model": {
            "type": "string",
            "description": "Model identifier"
          },
          "hash": {
            "type": "string",
            "description": "Adapter hash"
          },
          "input_format": {
            "type": "string",
            "description": "Input format specification"
          },
          "output_format": {
            "type": "string",
            "description": "Output format specification"
          },
          "config": {
            "type": "object",
            "description": "Adapter configuration"
          },
          "created_at": {
            "type": "string",
            "format": "date-time",
            "description": "Creation timestamp"
          },
          "updated_at": {
            "type": "string",
            "format": "date-time",
            "description": "Last update timestamp"
          }
        },
        "required": ["adapter_id", "name", "version", "domain_type", "model", "hash", "created_at", "updated_at"]
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

#### Get Historical System Metrics
```bash
# Get last 24 hours of metrics (default)
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8080/api/v1/metrics/system/history"

# Get last 48 hours, limited to 500 records
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8080/api/v1/metrics/system/history?hours=48&limit=500"

# Response: Array of historical metrics
# [
#   {
#     "id": 1,
#     "timestamp": 1640995200,
#     "cpu_usage": 45.2,
#     "memory_usage": 62.8,
#     "disk_read_bytes": 1024000,
#     "disk_write_bytes": 512000,
#     "disk_usage_percent": 23.1,
#     "network_rx_bytes": 2048000,
#     "network_tx_bytes": 1536000,
#     "network_rx_packets": 1500,
#     "network_tx_packets": 1200,
#     "network_bandwidth_mbps": 1.2,
#     "gpu_utilization": 15.5,
#     "gpu_memory_used": 1048576,
#     "gpu_memory_total": 8589934592,
#     "uptime_seconds": 86400,
#     "process_count": 156,
#     "load_1min": 1.2,
#     "load_5min": 1.1,
#     "load_15min": 1.0
#   }
# ]
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
  -d '{"email":"admin@example.com","password":"password"}'

# Use token in subsequent requests
curl -H "Authorization: Bearer <token>" \
  http://localhost:8080/api/v1/adapters
```

### Domain Adapters

#### Create a Code Analysis Adapter
```bash
curl -X POST http://localhost:8080/api/v1/domain-adapters \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Rust Code Analyzer",
    "version": "1.0.0",
    "domain_type": "code",
    "model": "deterministic-code-analyzer",
    "description": "Analyzes Rust code for patterns and quality",
    "input_format": "rust_source",
    "output_format": "analysis_json"
  }'
```

#### Load and Execute Code Analysis
```bash
# Load adapter
curl -X POST http://localhost:8080/api/v1/domain-adapters/{adapter_id}/load \
  -H "Authorization: Bearer <token>"

# Execute analysis
curl -X POST http://localhost:8080/api/v1/domain-adapters/{adapter_id}/execute \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{
    "code": "fn hello() { println!(\"Hello!\"); }",
    "language": "rust"
  }'
```

#### Test Adapter Determinism
```bash
curl -X POST http://localhost:8080/api/v1/domain-adapters/{adapter_id}/test \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{
    "input_data": "{\"code\":\"fn test(){}\",\"language\":\"rust\"}",
    "iterations": 50
  }'
```

## API Endpoints Summary

The API provides comprehensive endpoints for:

- **Authentication** - Login and JWT token management
- **Adapters** - Register, list, and manage adapters
- **Domain Adapters** - Deterministic domain-specific processing (code, vision, text, audio, multimodal)
- **Models** - Import and manage base models
- **Training** - Training job management and monitoring
- **Inference** - Chat completions with adapter selection
- **Metrics** - System metrics, historical data, and adapter performance metrics
- **Repositories** - Git repository management and scanning
- **Health** - Health and readiness checks

Generated on 2025-01-15
Generated on Tue Oct 14 08:33:47 CDT 2025
