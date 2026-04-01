# Token Compress Engine Architecture

## Project Identity

```
Project  : token_compress_engine
Stack    : Rust | Axum | SQLite
Scale    : Monolithic service, designed for millions of records
Owner    : navinvaddey
Last updated : 2026-03-29 by kilo/nvidia/nemotron-3-super-120b-a12b:free
```

## Architecture Overview

The Token Compress Engine is a monolithic Rust application built with the Axum web framework and backed by SQLite. It provides token compression capabilities through various algorithms and includes administrative monitoring features.

### High-Level Component Diagram

```
┌─────────────────┐    ┌──────────────────┐    ┌────────────────────┐
│   HTTP Layer    │    │ Business Logic   │    │   Data Access      │
│ (Controllers)   │───▶│ (Services/Domain)│───▶│ (Repositories/DB)  │
└─────────────────┘    └──────────────────┘    └────────────────────┘
         │                         │                         │
         ▼                         ▼                         ▼
┌─────────────────┐    ┌──────────────────┐    ┌────────────────────┐
│   Static Files  │    │ Learning Engine  │    │   SQLite Database  │
│   (Frontend)    │    │ (Background)     │    │                    │
└─────────────────┘    └──────────────────┘    └────────────────────┘
```

### Directory Structure

```
src/
├── api/              # HTTP controllers and route definitions
├── data/             # Database access layer (repositories)
├── domain/           # Business logic and use cases
├── engine/           # Core compression algorithms and learning engine
├── behavior/         # User behavior analysis and feedback processing
├── models/           # Data models and database schema
├── algorithms/       # Specific compression algorithm implementations
├── pipeline/         # Data processing pipeline orchestration
├── session/          # User session management
├── types.rs          # Shared types and constants
└── lib.rs            # Library exports and shared state (AppState)
```

## Component Details

### 1. API Layer (`src/api/`)
Handles HTTP requests and responses, routing, and middleware.
- Defines API endpoints using Axum
- Handles request validation and response formatting
- Contains authentication and authorization logic

### 2. Data Layer (`src/data/`)
Database access layer implementing the repository pattern.
- Contains database connection and query logic
- Implements CRUD operations for all entities
- Uses SQLx for database interactions

### 3. Domain Layer (`src/domain/`)
Contains business logic and application use cases.
- Orchestrates data flow between repositories and engines
- Implements core application logic (user registration, compression, etc.)
- Coordinates with the learning engine for adaptive behavior

### 4. Engine Layer (`src/engine/`)
Implements the core token compression algorithms and learning capabilities.
- LearningEngine: Maintains and updates compression models
- Various algorithm implementations (predictive coding, sparse coding, etc.)
- Pipeline orchestration for multi-stage compression
- Evaluation metrics for algorithm performance

### 5. Behavior Layer (`src/behavior/`)
Analyzes user behavior and feedback to improve compression.
- Feedback detection and processing
- User interaction analysis
- Adaptive learning based on usage patterns

### 6. Models Layer (`src/models/`)
Defines data structures and database schema.
- User, compression history, feedback signals, etc.
- Database table mappings via SQLx
- Validation logic for data integrity

### 7. Algorithms Layer (`src/algorithms/`)
Specific implementations of compression techniques.
- Lexical compression
- Semantic clustering
- Schema filling
- Working memory optimization
- Predictive and sparse coding

### 8. Pipeline Layer (`src/pipeline/`)
Orchestrates the flow of data through compression stages.
- Pipeline configuration and execution
- Stage management and error handling
- Resource allocation and optimization

### 9. Supporting Modules
- `session.rs`: User session management
- `types.rs`: Shared constants and type definitions
- `errors.rs`: Custom error types and handling
- `lib.rs`: Library exports and shared AppState

## Data Flow

1. **Request Handling**: HTTP requests arrive at the API layer
2. **Authentication**: Middleware validates user credentials
3. **Business Logic**: Controllers delegate to domain services
4. **Data Access**: Services interact with repositories for DB operations
5. **Processing**: Core algorithms process data in the engine layer
6. **Learning**: Feedback is processed to improve future compression
7. **Response**: Results are returned through the API layer

## Configuration

- Environment variables loaded via `dotenvy`
- Database connection configured through `DATABASE_URL`
- Admin credentials set via `ADMIN_PASSWORD`
- Logging configured with `tracing` and `tracing-subscriber`

## Change Log (Architectural Changes)

```
[2026-03-29] [Model: kilo/nvidia/nemotron-3-super-120b-a12b:free] [File: ARCHITECTURE.md]
What changed : Created initial architecture documentation with component diagrams and details
Why          : Provide clear architectural overview for development and maintenance
Impact       : Establishes baseline for tracking future architectural decisions
```