RPEngineProfilesDB = {
    ["literal"] = "braces { } quote \" slash \\\\ newline \n",
    nested = { levels = { 1, 2, 3 } },
}
RPEngineDatasetDB = {
    ["opaque"] = { ["text"] = "dataset {content} with \"quotes\" and \\\\ escapes" },
}

RPEngineManagerDB = {
    protocolVersion = 1,
    pendingOperations = {
        {
            requestId = "550e8400-e29b-41d4-a716-446655440000",
            operation = "install_dataset",
            catalogueId = "esarus-core",
            datasetId = "f82db71a",
            revision = 14,
            hash = "2aee76c57aa954b6ca769bb1c1ff839c0805cea4aa4ffb3ebe153fe48e3c71de",
            payload = "RPE_DATASET_V1\nfixture payload\n",
        },
    },
    installedPackages = {
        ["esarus-core"] = {
            packageType = "dataset",
            datasetId = "f82db71a",
            revision = 13,
            hash = "2aee76c57aa954b6ca769bb1c1ff839c0805cea4aa4ffb3ebe153fe48e3c71de",
            installedAt = 1789940000,
        },
    },
    operationResults = {
        ["550e8400-e29b-41d4-a716-446655440002"] = {
            requestId = "550e8400-e29b-41d4-a716-446655440002",
            operation = "install_dataset",
            status = "failed",
            catalogueId = "esarus-core",
            datasetId = "f82db71a",
            revision = 12,
            hash = "2aee76c57aa954b6ca769bb1c1ff839c0805cea4aa4ffb3ebe153fe48e3c71de",
            error = {
                code = "import_rejected",
                detail = "Importer rejected text containing {braces}, quotes \\\" and escapes \\\\\\\\.",
            },
        },
    },
}

RPEngineInventoryDB = {
    ["untouched"] = { nested = { [1] = "inventory } value" } },
}
