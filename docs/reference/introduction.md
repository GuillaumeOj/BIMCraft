# Introduction

This book is the reference of what a BIMCraft model holds, for developers of plugins, scripts and integrations that read or write models through the API. To learn how to use BIMCraft, read the user guide instead.

The schema pages describe every entity (the kinds of object a model holds, such as walls or storeys) and every property set (named groups of properties attached to them), with their attributes, IFC mapping and the schema version that introduced them. They are generated from the schema definitions in `schema/`, so they always match the release they are published with; the JSON Schema of the same data is `schema/core/api.schema.json`.
