# Runsite CLI

Command-line and terminal UI client for the Runsite hosting platform: manage projects, services, databases, deployments and environment variables.

## Language

### Platform resources

**Project**:
A named group of services and databases on the platform; one project can be selected as the current context.

**Service**:
A deployable application running inside a project.

**Database**:
A managed database instance (PostgreSQL, Redis/Valkey) inside a project.

**Deployment**:
One build-and-release attempt of a service.

**Profile**:
A saved set of credentials and API URL, plus the selected project.
_Avoid_: account, config

**Status**:
What the API reports a **Service**, **Database** or **Deployment** is doing, spelled as the API spells it (`running`, `pending_deletion`, ...). A status the CLI does not know yet is kept as is, never an error.

**Condition**:
What a **Status** means, the same for every resource: running, stopped, sleeping, busy, failed, blocked, deleting or unknown. The TUI's glyphs, the CLI's colours and which actions are allowed are all decided from the **Condition**, never from the raw **Status**.
_Avoid_: state, phase, look

### CLI input and output

**Reference**:
What the user types to name a service, database or project: its ID or its name.
_Avoid_: target, selector, identifier

**Report**:
The value a CLI command returns, which can be rendered either as JSON or as human-readable text.
_Avoid_: result, output, response

## Relationships

- A **Profile** selects at most one **Project**
- A **Project** contains many **Services** and **Databases**
- A **Service** has many **Deployments**
- A name **Reference** to a **Service** or **Database** is looked up inside the **Project** the **Profile** selects; an ID **Reference** matches in any **Project**
- `service list` and `db list` show the same scope a name **Reference** is looked up in; `--all` widens both lists
- Every CLI command produces exactly one **Report**; for a mutating command it is the resource the API returned, or a short `{id, action}` object when the API returns nothing

## Example dialogue

> **Dev:** "`runsite service restart` doesn't print anything useful in JSON mode — should it print the **Service**?"
> **Maintainer:** "Its **Report** is whatever the API returned. Restart answers with the service's runtime status, so the **Report** is `{service_id, action: "restart", status}`."

## Flagged ambiguities

- "output" meant both the `--output` format flag and what a command prints — resolved: the flag is the output format; what a command produces is its **Report**.
