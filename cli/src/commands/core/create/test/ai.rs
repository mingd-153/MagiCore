    #[test]
    fn dev_routes_to_entry_script() {
        assert_eq!(
            mg_ai_adapter::AiFramework::PythonAgent.entry_script(),
            "src/agent.py"
        );
        assert_eq!(
            mg_ai_adapter::AiFramework::McpServer.entry_script(),
            "server.py"
        );
    }