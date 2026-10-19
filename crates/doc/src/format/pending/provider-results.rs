//! A design saves the results geometry providers made for its features (`provider_results`: the
//! feature, the provider and its version, the operation, a digest of the design up to the feature,
//! what the operation's operands were like, and the result as STEP text, a blob in a container), so
//! a computer without the provider still shows, measures and exports them. Older designs have none
//! and need no step. The last release refuses a design saved with this (the newer format number).
