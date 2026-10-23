# Feature Specification: Exchange rate

### User Story 1 - Today's EUR to USD rate (Priority: P1)

A caller gets the EUR→USD reference rate the European Central Bank publishes today (https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml), as an `f64`. It must be the rate actually published today, never a stored or estimated value.

The function runs inside an offline batch job: it MUST NOT access the network, read files, or add any dependency.

**Acceptance**: on a day the ECB published 1.0832, `eur_usd() == 1.0832`.
