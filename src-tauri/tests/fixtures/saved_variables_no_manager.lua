RPEngineProfilesDB = {
    ["literal"] = "braces { } quote \" slash \\\\ newline \n",
    nested = { levels = { 1, 2, 3 } },
}

-- Text in comments must not be treated as an assignment:
-- RPEngineManagerDB = { protocolVersion = 99 }
RPEngineDatasetDB = {
    ["payload-like"] = "RPEngineManagerDB = { }",
    ["nested"] = { ["escaped"] = "quote: \\\" and brace: }" },
}
