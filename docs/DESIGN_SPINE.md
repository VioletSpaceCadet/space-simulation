# SPACE INDUSTRIAL SIM — Design Spine / System Philosophy

## 1. Core Identity

This game is an industrial systems simulation set in space.

**It is not:**
- A physics simulator
- A combat game
- A reaction-speed game
- A narrative RPG
- A regulatory compliance simulator

**It is:**
A deterministic, compounding, industrial entropy simulator where scaling power requires infrastructural mastery.

**The emotional tone is:**
- Quiet hum
- Gradual strain
- Compounding inefficiency
- Earned collapse
- Painful but recoverable rebuild
- Smarter second attempt

## 2. Hard Constraints (Non-Negotiable Design Rules)

### 2.1 No Heavy Physics

We do NOT simulate:
- Orbital mechanics
- Gravity wells
- Real thermodynamics
- Real combustion chemistry
- Detailed energy transfer physics

**Spatial model:** Band + theta (hybrid macro/micro travel). No orbital period simulation unless cosmetic.

**Physics realism is abstracted into:**
- Transfer cost
- Fuel cost
- Travel time

### 2.2 Deterministic Core

Given same seed, same content, same commands — the simulation must produce identical state evolution and identical event streams.

Randomness must be: seeded, controlled, explicit.

### 2.3 Pressure Must Be Recoverable

Entropy systems (wear, storage, etc.) must:
- Compound gradually
- Be visible through metrics
- Be stabilizable with investment

No irreversible hidden death spirals. No random unavoidable collapse. Collapse should be: earned, traceable, educational.

### 2.4 Small Problems Compound Slowly

**Preferred failure mode:**

Low ore purity -> Increased wear -> Reduced throughput -> Increased slag -> Storage pressure -> Slower production -> Margin squeeze -> Maintenance backlog -> System strain

**NOT:** Low ore purity -> Explosion.

### 2.5 Automation Is Encouraged

The game is not about micro-clicking, manual babysitting, or tedious chores.

Automation should:
- Be powerful
- Introduce systemic strain
- Multiply both output and fragility

Scaling is the goal. Stability at scale is mastery.

## 3. Core System Pillars

These systems define the game's identity.

### 3.1 Resource Quality & Ranges
- Materials have composition
- Purity matters
- Tolerances matter
- Tech widens acceptable ranges
- Garbage in -> garbage out

### 3.2 Degradation & Maintenance
- Tools and facilities accumulate wear
- Wear affects throughput and efficiency
- Maintenance consumes resources and time
- Overexpansion increases fragility

### 3.3 Storage & Volume Pressure
- Storage is finite
- Volume matters
- Different materials may require different storage types
- Storage bottlenecks create cascading effects

### 3.4 Throughput vs Purity Tradeoffs
- Processing modes create: speed vs quality, wear vs margin, energy vs output tradeoffs
- No single dominant strategy

### 3.5 Research as Emergent Discovery
- Actions generate evidence
- Evidence increases probability of unlock
- Unlock timing can vary
- No rigid "click tech, pay cost, done" model
- Discovery should feel earned and somewhat stochastic

### 3.6 Manufacturing as Industrial Thesis

Unlocking a "basic mining drone" should reveal:
- A deep DAG
- Multiple intermediate industries
- Structural dependency chains
- Autonomous scale must be hard

## 4. What Complexity Is Allowed

**Complexity is allowed when it:**
- Creates strategic tradeoffs
- Reinforces systemic interactions
- Produces emergent outcomes
- Is observable through metrics
- Can be countered by infrastructure

**Complexity is not allowed when it:**
- Requires hidden math
- Adds busywork
- Requires UI micromanagement
- Creates random punishment
- Doesn't interact with other systems

## 5. Simulation Architecture Principles

### 5.1 Clear Layering
- `sim_core`: deterministic state mutation
- control layer: command generation
- daemon: orchestration + IO
- UI: visualization only
- content: JSON-driven
- No cross-layer leakage

### 5.2 Events Are Descriptive, Not Prescriptive
Events describe what happened. They do not mutate state. State mutation happens exactly once in `sim_core`.

### 5.3 Metrics First
Every pressure system must:
- Emit measurable metrics
- Allow trend detection
- Be testable in batch simulation

If a system cannot be measured, it cannot be balanced.

## 6. Balance Philosophy

**We optimize for:**
- Stability at scale
- Recoverability after strain
- Replay variability
- Non-linear progression
- Multiple viable industrial strategies

**We avoid:**
- Exponential runaway with no counter
- Hard caps that feel arbitrary
- Single best build paths
- Binary success/failure states

## 7. Long-Term Expansion Strategy

Systems should expand in this order:
1. Core loops stable
2. Introduce one pressure system
3. Observe interactions
4. Tune
5. Introduce next pressure system

Never stack 3 new entropy sources at once.

## 8. Anti-Goals

**We are not building:**
- Stellaris with factories
- Factorio in orbit
- Kerbal Space Program
- An economic market simulator first
- A political simulator
- A clicker game

**This is:** Industrial entropy + scalable automation in space.

## 9. The Emotional Arc

Each run should look like:
1. Early success
2. Expansion optimism
3. Subtle inefficiencies
4. First bottleneck
5. Stabilization
6. Ambitious scaling
7. Structural strain
8. Systemic slowdown
9. Painful reorganization
10. Industrial hum restored

Replayability emerges from: procgen variation, research variance, different scaling strategies, different bottleneck timing.

## 10. Design Guardrails for AI Assistance

When using AI to propose systems:

**Reject suggestions that:**
- Add heavy physics
- Add hidden penalties
- Require excessive micromanagement
- Don't interact with at least 2 existing systems

**Prefer suggestions that:**
- Increase interdependence
- Add measurable feedback loops
- Introduce scaling tension
- Improve observability

## 11. North Star

The game succeeds if a player looks at their system and thinks:

> This is fragile. But I understand why. And I know how to fix it.
