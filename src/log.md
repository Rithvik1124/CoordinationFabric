I'd separate this into **state** and **assignment logic**. Don't try to determine roles inside the `HashMap` itself.

## 1. Model the roles

Don't use strings.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Role {
    Sigma,
    Consensus,
    IOC,
}
```

If a node can hold multiple roles (which it sounds like it can), store them in a `Vec<Role>` or `HashSet<Role>`.

```rust
struct NodeInfo {
    perf: CpuPayload,
    roles: Vec<Role>,
}
```

Then your global state is:

```rust
HashMap<IpAddr, NodeInfo>
```

---

## 2. Decide on a scoring function

Your `avg_score` is probably already intended to represent overall compute capability.

If not, define one.

For example:

```rust
fn score(cpu: &CpuPayload) -> f64 {
    cpu.avg_score
}
```

or

```rust
fn score(cpu: &CpuPayload) -> f64 {
    cpu.avg_score * (1.0 - cpu.cpu_usage)
}
```

depending on whether `avg_score` is a benchmark or a live measurement.

---

## 3. Every time a node joins

Recompute the assignments.

Don't try to "incrementally" assign them.

```text
insert node
↓
sort all nodes
↓
assign roles
```

This is simple and deterministic.

---

## 4. Sort by score

```rust
let mut nodes: Vec<_> = map.iter_mut().collect();

nodes.sort_by(|a, b| {
    b.1.perf.avg_score
        .partial_cmp(&a.1.perf.avg_score)
        .unwrap()
});
```

Now

```
nodes[0] -> strongest
nodes[1] -> second strongest
nodes[2] -> third strongest
```

---

## 5. Clear all roles

```rust
for (_, node) in &mut nodes {
    node.roles.clear();
}
```

---

## 6. Assign according to the number of nodes

### One node

```
Sigma
Consensus
IOC
```

```text
Node A
 ├ Sigma
 ├ Consensus
 └ IOC
```

---

### Two nodes

Highest score:

```
Sigma
Consensus
```

Second:

```
IOC
```

or

Highest:

```
Sigma
```

Second:

```
Consensus
IOC
```

Choose whichever balances your workload better.

---

### Three or more

```
Highest -> Sigma

Second -> Consensus

Third -> IOC
```

Everyone else gets no special role.

---

## 7. Wrap it in one function

Something like

```rust
fn rebalance_roles(map: &mut HashMap<IpAddr, NodeInfo>)
```

whose job is:

```
clear roles

sort nodes

assign roles
```

Then every insertion is just

```
insert node

rebalance_roles()
```

---

## Example

Suppose

```
A = 98
B = 87
C = 74
D = 53
```

Assignments become

```
A -> Sigma

B -> Consensus

C -> IOC

D -> none
```

If A leaves

```
B -> Sigma

C -> Consensus

D -> IOC
```

No special logic is needed; you just rerun the assignment.

## Why this approach?

This is essentially a scheduler. Schedulers are much easier to reason about if they compute the desired state from the current cluster state rather than trying to patch assignments incrementally after every event. The algorithm is only (O(n \log n)) because of the sort, which is negligible unless you're dealing with a very large number of nodes. For a cluster with dozens or even hundreds of nodes, recomputing roles whenever membership changes is simple, predictable, and easy to maintain.
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
I still think this is the cleanest architecture, but I'd separate **role assignment** from **task scheduling**. They solve different problems.

```
                Membership Changes
                        │
                        ▼
              Role Determination Engine
                        │
                        ▼
          Current Cluster Configuration
                        │
                        ▼
                 Task Scheduler
                        │
                        ▼
                  Endpoint Execution
```

The role engine only answers:

> "Who is Sigma? Who is Consensus? Who is IOC?"

The scheduler answers:

> "Given these roles, where should this task run?"

---

## Step 1: Cluster state

Instead of storing only the roles, I'd also store whether a node is currently alive.

```rust
use std::{
    collections::HashMap,
    net::IpAddr,
};

#[derive(Debug, Clone)]
pub struct CpuPayload {
    pub avg_score: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Sigma,
    Consensus,
    IOC,
}

#[derive(Debug)]
pub struct NodeInfo {
    pub perf: CpuPayload,
    pub roles: Vec<Role>,
    pub alive: bool,
}

pub type Cluster = HashMap<IpAddr, NodeInfo>;
```

---

# Step 2: Role policy

Your assignment policy can live entirely in one function.

```rust
fn rebalance_roles(cluster: &mut Cluster) {
    let mut nodes: Vec<_> = cluster
        .iter_mut()
        .filter(|(_, n)| n.alive)
        .collect();

    nodes.sort_by(|a, b| {
        b.1.perf
            .avg_score
            .partial_cmp(&a.1.perf.avg_score)
            .unwrap()
    });

    for (_, node) in &mut nodes {
        node.roles.clear();
    }

    match nodes.len() {
        0 => {}

        1 => {
            nodes[0].1.roles.extend([
                Role::Sigma,
                Role::Consensus,
                Role::IOC,
            ]);
        }

        2 => {
            nodes[0].1.roles.extend([
                Role::Sigma,
                Role::Consensus,
            ]);

            nodes[1].1.roles.push(Role::IOC);
        }

        _ => {
            nodes[0].1.roles.push(Role::Sigma);
            nodes[1].1.roles.push(Role::Consensus);
            nodes[2].1.roles.push(Role::IOC);
        }
    }
}
```

Notice there is no scheduler here.

---

# Step 3: Scheduler

The scheduler doesn't care about performance anymore.

It only looks at roles.

Suppose every incoming task has a required role.

```rust
enum TaskRole {
    Sigma,
    Consensus,
    IOC,
}
```

Then

```rust
struct Task {
    id: u64,
    role: TaskRole,
}
```

Routing is straightforward.

```rust
fn find_executor<'a>(
    cluster: &'a Cluster,
    role: Role,
) -> Option<IpAddr> {

    cluster
        .iter()
        .find(|(_, node)| {
            node.alive &&
            node.roles.contains(&role)
        })
        .map(|(ip, _)| *ip)
}
```

Usage

```rust
let endpoint = find_executor(
    &cluster,
    Role::Consensus,
);
```

---

# Step 4: What if every task must run everywhere?

This is where the distinction becomes important.

From your description:

> every endpoint has to do all the tasks

That isn't scheduling in the traditional sense.

It's replication.

For example:

```
Task 42
```

Initially

```
Node A

Sigma
Consensus
IOC

runs Task 42
```

A second node joins.

```
Node A
Sigma

Node B
Consensus
IOC
```

Now Task 43 becomes

```
Task 43

Sigma  -> Node A

Consensus -> Node B

IOC -> Node B
```

Notice Task 43 isn't executed once.

It is executed **once per role**.

---

# Better abstraction

Instead of scheduling to **nodes**, schedule to **roles**.

```
Task

↓

Required Roles

↓

Role Engine

↓

Node(s)

↓

Execution
```

For example

```rust
enum TaskKind {
    SigmaOnly,
    ConsensusOnly,
    IOCOnly,
    All,
}
```

Then

```rust
match task.kind {
    TaskKind::SigmaOnly => { ... }

    TaskKind::ConsensusOnly => { ... }

    TaskKind::IOCOnly => { ... }

    TaskKind::All => {
        dispatch(Role::Sigma);
        dispatch(Role::Consensus);
        dispatch(Role::IOC);
    }
}
```

The dispatcher resolves each role to its current node.

---

# Scaling further

This also scales naturally to more complex policies.

Suppose you later decide:

```
Top 2 nodes
↓

Sigma

Next 3 nodes
↓

Consensus

Everyone else
↓

IOC
```

You only change `rebalance_roles()`.

The scheduler remains exactly the same because it only asks:

> "Who currently owns the Sigma role?"

It never needs to know *why* that node owns it. This separation of concerns makes the system easier to extend as the cluster grows or the assignment policy becomes more sophisticated.
