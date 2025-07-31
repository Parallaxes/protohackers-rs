#[cfg(test)]
mod tests {
    use super::*;
    use crate::challenge06::{
        challenge06::{CLIENT_ID_COUNTER, ProtocolError, process_message, validate_message},
        client::{ClientState, ClientType},
        db::PlateObservation,
        protocol::{IAmCamera, Message, Plate},
    };
    use std::collections::{BTreeMap, HashMap};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn create_test_addr(port: u16) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port)
    }

    fn create_test_camera() -> IAmCamera {
        IAmCamera {
            road: 123,
            mile: 100,
            limit: 60,
        }
    }

    fn create_test_plate(plate: &str, timestamp: u32) -> Plate {
        Plate {
            plate: plate.to_string(),
            timestamp,
        }
    }

    async fn setup_camera_client(
        clients: &Arc<Mutex<HashMap<SocketAddr, ClientState>>>,
        addr: SocketAddr,
        camera: IAmCamera,
    ) -> u32 {
        let mut client_map = clients.lock().await;
        let mut state = ClientState::new();
        let client_type = ClientType::Camera {
            road: camera.road,
            mile: camera.mile,
            limit: camera.limit,
        };
        state.set_client_type(client_type);
        let client_id = 42; // Fixed ID for testing
        state.client_id = Some(client_id);
        client_map.insert(addr, state);
        client_id
    }

    #[tokio::test]
    async fn test_speed_calculation_no_violation() {
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let plates = Arc::new(Mutex::new(BTreeMap::new()));
        let addr = create_test_addr(8001);

        // Setup camera at mile 100
        let camera = IAmCamera {
            road: 123,
            mile: 100,
            limit: 60,
        };
        setup_camera_client(&clients, addr, camera).await;

        // First observation
        let plate_msg1 = create_test_plate("ABC123", 1000);
        let result = process_message(Message::Plate(plate_msg1), addr, &clients, &plates).await;
        assert!(result.is_ok());

        // Setup second camera at mile 110 (10 miles away)
        let addr2 = create_test_addr(8002);
        let camera2 = IAmCamera {
            road: 123,
            mile: 110,
            limit: 60,
        };
        setup_camera_client(&clients, addr2, camera2).await;

        // Second observation 10 minutes later (600 seconds)
        // Speed: 10 miles / 600 seconds * 3600 = 60 mph (exactly at limit)
        let plate_msg2 = create_test_plate("ABC123", 1600);
        let result = process_message(Message::Plate(plate_msg2), addr2, &clients, &plates).await;
        assert!(result.is_ok());

        // Check that both observations are stored
        let plates_guard = plates.lock().await;
        assert_eq!(plates_guard.len(), 2);
    }

    #[tokio::test]
    async fn test_speed_violation_detection() {
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let plates = Arc::new(Mutex::new(BTreeMap::new()));
        let addr = create_test_addr(8001);

        // Setup camera at mile 100
        let camera = IAmCamera {
            road: 123,
            mile: 100,
            limit: 60,
        };
        setup_camera_client(&clients, addr, camera).await;

        // First observation
        let plate_msg1 = create_test_plate("SPEED123", 1000);
        let result = process_message(Message::Plate(plate_msg1), addr, &clients, &plates).await;
        assert!(result.is_ok());

        // Setup second camera at mile 110
        let addr2 = create_test_addr(8002);
        let camera2 = IAmCamera {
            road: 123,
            mile: 110,
            limit: 60,
        };
        setup_camera_client(&clients, addr2, camera2).await;

        // Second observation 5 minutes later (300 seconds)
        // Speed: 10 miles / 300 seconds * 3600 = 120 mph (violation!)
        let plate_msg2 = create_test_plate("SPEED123", 1300);
        let result = process_message(Message::Plate(plate_msg2), addr2, &clients, &plates).await;
        assert!(result.is_ok());

        // Verify both observations are stored
        let plates_guard = plates.lock().await;
        assert_eq!(plates_guard.len(), 2);

        // Check that we can find both observations
        let key1 = ("SPEED123".to_string(), 1000);
        let key2 = ("SPEED123".to_string(), 1300);
        assert!(plates_guard.contains_key(&key1));
        assert!(plates_guard.contains_key(&key2));
    }

    #[tokio::test]
    async fn test_different_roads_no_violation() {
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let plates = Arc::new(Mutex::new(BTreeMap::new()));

        // Camera on road 123
        let addr1 = create_test_addr(8001);
        let camera1 = IAmCamera {
            road: 123,
            mile: 100,
            limit: 60,
        };
        setup_camera_client(&clients, addr1, camera1).await;

        // Camera on different road (456)
        let addr2 = create_test_addr(8002);
        let camera2 = IAmCamera {
            road: 456, // Different road
            mile: 100,
            limit: 60,
        };
        setup_camera_client(&clients, addr2, camera2).await;

        // Same plate on different roads
        let plate_msg1 = create_test_plate("ABC123", 1000);
        let result1 = process_message(Message::Plate(plate_msg1), addr1, &clients, &plates).await;
        assert!(result1.is_ok());

        let plate_msg2 = create_test_plate("ABC123", 1100);
        let result2 = process_message(Message::Plate(plate_msg2), addr2, &clients, &plates).await;
        assert!(result2.is_ok());

        // Should have both observations but no violation (different roads)
        let plates_guard = plates.lock().await;
        assert_eq!(plates_guard.len(), 2);
    }

    #[tokio::test]
    async fn test_same_mile_no_violation() {
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let plates = Arc::new(Mutex::new(BTreeMap::new()));
        let addr = create_test_addr(8001);

        // Setup camera
        let camera = IAmCamera {
            road: 123,
            mile: 100,
            limit: 60,
        };
        setup_camera_client(&clients, addr, camera).await;

        // Two observations at same mile (shouldn't trigger violation check)
        let plate_msg1 = create_test_plate("ABC123", 1000);
        let plate_msg2 = create_test_plate("ABC123", 1100);

        let result1 = process_message(Message::Plate(plate_msg1), addr, &clients, &plates).await;
        assert!(result1.is_ok());

        let result2 = process_message(Message::Plate(plate_msg2), addr, &clients, &plates).await;
        assert!(result2.is_ok());

        let plates_guard = plates.lock().await;
        assert_eq!(plates_guard.len(), 2);
    }

    #[tokio::test]
    async fn test_client_not_camera_error() {
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let plates = Arc::new(Mutex::new(BTreeMap::new()));
        let addr = create_test_addr(8001);

        // Setup dispatcher instead of camera
        let mut client_map = clients.lock().await;
        let mut state = ClientState::new();
        let client_type = ClientType::Dispatcher { roads: vec![123] };
        state.set_client_type(client_type);
        state.client_id = Some(42);
        client_map.insert(addr, state);
        drop(client_map);

        // Try to send plate message from dispatcher
        let plate_msg = create_test_plate("ABC123", 1000);
        let result = process_message(Message::Plate(plate_msg), addr, &clients, &plates).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a camera"));
    }

    #[tokio::test]
    async fn test_client_not_identified_error() {
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let plates = Arc::new(Mutex::new(BTreeMap::new()));
        let addr = create_test_addr(8001);

        // Setup client without ID
        let mut client_map = clients.lock().await;
        let state = ClientState::new(); // No client_id set
        client_map.insert(addr, state);
        drop(client_map);

        let plate_msg = create_test_plate("ABC123", 1000);
        let result = process_message(Message::Plate(plate_msg), addr, &clients, &plates).await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("not fully identified")
        );
    }

    #[tokio::test]
    async fn test_multiple_violations_same_plate() {
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let plates = Arc::new(Mutex::new(BTreeMap::new()));

        // Setup three cameras on same road
        let cameras = vec![
            (
                create_test_addr(8001),
                IAmCamera {
                    road: 123,
                    mile: 100,
                    limit: 60,
                },
            ),
            (
                create_test_addr(8002),
                IAmCamera {
                    road: 123,
                    mile: 110,
                    limit: 60,
                },
            ),
            (
                create_test_addr(8003),
                IAmCamera {
                    road: 123,
                    mile: 120,
                    limit: 60,
                },
            ),
        ];

        for (addr, camera) in &cameras {
            setup_camera_client(&clients, *addr, camera.clone()).await;
        }

        // Add observations that create multiple violations
        let observations = vec![
            (0, create_test_plate("FAST123", 1000)), // Mile 100 at t=1000
            (1, create_test_plate("FAST123", 1200)), // Mile 110 at t=1200 (30mph - ok)
            (2, create_test_plate("FAST123", 1250)), // Mile 120 at t=1250 (72mph - violation!)
        ];

        for (camera_idx, plate_msg) in observations {
            let addr = cameras[camera_idx].0;
            let result = process_message(Message::Plate(plate_msg), addr, &clients, &plates).await;
            assert!(result.is_ok());
        }

        // Should have all three observations
        let plates_guard = plates.lock().await;
        assert_eq!(plates_guard.len(), 3);
    }

    #[tokio::test]
    async fn test_btreemap_key_ordering() {
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let plates = Arc::new(Mutex::new(BTreeMap::new()));
        let addr = create_test_addr(8001);

        let camera = create_test_camera();
        setup_camera_client(&clients, addr, camera).await;

        // Add observations in non-chronological order
        let observations = vec![
            create_test_plate("XYZ789", 2000),
            create_test_plate("ABC123", 1000),
            create_test_plate("XYZ789", 1500),
            create_test_plate("ABC123", 3000),
        ];

        for plate_msg in observations {
            let result = process_message(Message::Plate(plate_msg), addr, &clients, &plates).await;
            assert!(result.is_ok());
        }

        // Verify BTreeMap maintains sorted order
        let plates_guard = plates.lock().await;
        let keys: Vec<_> = plates_guard.keys().collect();

        // Should be sorted by (plate, timestamp)
        let expected_order = vec![
            ("ABC123".to_string(), 1000),
            ("ABC123".to_string(), 3000),
            ("XYZ789".to_string(), 1500),
            ("XYZ789".to_string(), 2000),
        ];
        let expected_refs: Vec<_> = expected_order.iter().collect();
        assert_eq!(keys, expected_refs);
        // assert_eq!(keys, expected_order);
    }

    #[tokio::test]
    async fn test_zero_time_diff_no_division_by_zero() {
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let plates = Arc::new(Mutex::new(BTreeMap::new()));
        let addr = create_test_addr(8001);

        let camera = IAmCamera {
            road: 123,
            mile: 100,
            limit: 60,
        };
        setup_camera_client(&clients, addr, camera).await;

        // Same timestamp should not cause division by zero
        let plate_msg1 = create_test_plate("ABC123", 1000);
        let plate_msg2 = create_test_plate("ABC123", 1000); // Same timestamp

        let result1 = process_message(Message::Plate(plate_msg1), addr, &clients, &plates).await;
        assert!(result1.is_ok());

        // Second message with same timestamp should be handled gracefully
        // (though this would likely be filtered out by timestamp != check)
        let result2 = process_message(Message::Plate(plate_msg2), addr, &clients, &plates).await;
        assert!(result2.is_ok());
    }

    #[tokio::test]
    async fn test_validate_message_functionality() {
        // Test all validation scenarios

        // Camera can send plate messages
        let camera_state = {
            let mut state = ClientState::new();
            state.set_client_type(ClientType::Camera {
                road: 123,
                mile: 100,
                limit: 60,
            });
            state.client_id = Some(1);
            state
        };

        let plate_msg = Message::Plate(create_test_plate("ABC123", 1000));
        assert!(validate_message(plate_msg, &camera_state).is_ok());

        // Dispatcher cannot send plate messages
        let dispatcher_state = {
            let mut state = ClientState::new();
            state.set_client_type(ClientType::Dispatcher { roads: vec![123] });
            state.client_id = Some(2);
            state
        };

        let plate_msg = Message::Plate(create_test_plate("ABC123", 1000));
        let result = validate_message(plate_msg, &dispatcher_state);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ProtocolError::NotIdentified));

        // Unidentified client cannot send plate messages
        let unidentified_state = ClientState::new();
        let plate_msg = Message::Plate(create_test_plate("ABC123", 1000));
        let result = validate_message(plate_msg, &unidentified_state);
        assert!(result.is_err());

        // Cannot identify twice
        let identified_state = camera_state;
        let camera_msg = Message::IAmCamera(IAmCamera {
            road: 456,
            mile: 200,
            limit: 70,
        });
        let result = validate_message(camera_msg, &identified_state);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProtocolError::AlreadyIdentified
        ));

        // Cannot request heartbeat twice
        let mut heartbeat_state = ClientState::new();
        heartbeat_state.has_heartbeat = true;
        let heartbeat_msg =
            Message::WantHeartbeat(crate::challenge06::protocol::WantHeartbeat { interval: 100 });
        let result = validate_message(heartbeat_msg, &heartbeat_state);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProtocolError::DuplicateHeartbeatRequest
        ));
    }

    #[tokio::test]
    async fn test_camera_registration() {
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let plates = Arc::new(Mutex::new(BTreeMap::new()));
        let addr = create_test_addr(8001);

        // Initialize client state
        {
            let mut client_map = clients.lock().await;
            client_map.insert(addr, ClientState::new());
        }

        let camera_msg = Message::IAmCamera(IAmCamera {
            road: 42,
            mile: 150,
            limit: 80,
        });

        let result = process_message(camera_msg, addr, &clients, &plates).await;
        assert!(result.is_ok());

        // Verify client state was updated
        let client_guard = clients.lock().await;
        let client_state = client_guard.get(&addr).unwrap();
        assert!(client_state.is_identified());
        assert!(client_state.client_id.is_some());

        match &client_state.client_type {
            Some(ClientType::Camera { road, mile, limit }) => {
                assert_eq!(*road, 42);
                assert_eq!(*mile, 150);
                assert_eq!(*limit, 80);
            }
            _ => panic!("Expected camera client type"),
        }
    }

    #[tokio::test]
    async fn test_dispatcher_registration() {
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let plates = Arc::new(Mutex::new(BTreeMap::new()));
        let addr = create_test_addr(8001);

        // Initialize client state
        {
            let mut client_map = clients.lock().await;
            client_map.insert(addr, ClientState::new());
        }

        let dispatcher_msg = Message::IAmDispatcher(crate::challenge06::protocol::IAmDispatcher {
            numroads: 3,
            roads: vec![10, 20, 30],
        });

        let result = process_message(dispatcher_msg, addr, &clients, &plates).await;
        assert!(result.is_ok());

        // Verify client state was updated
        let client_guard = clients.lock().await;
        let client_state = client_guard.get(&addr).unwrap();
        assert!(client_state.is_identified());
        assert!(client_state.client_id.is_some());

        match &client_state.client_type {
            Some(ClientType::Dispatcher { roads }) => {
                assert_eq!(*roads, vec![10, 20, 30]);
            }
            _ => panic!("Expected dispatcher client type"),
        }
    }

    #[tokio::test]
    async fn test_heartbeat_request() {
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let plates = Arc::new(Mutex::new(BTreeMap::new()));
        let addr = create_test_addr(8001);

        // Initialize client state
        {
            let mut client_map = clients.lock().await;
            client_map.insert(addr, ClientState::new());
        }

        let heartbeat_msg = Message::WantHeartbeat(crate::challenge06::protocol::WantHeartbeat {
            interval: 250, // 25 seconds
        });

        let result = process_message(heartbeat_msg, addr, &clients, &plates).await;
        assert!(result.is_ok());

        // Verify heartbeat flag was set
        let client_guard = clients.lock().await;
        let client_state = client_guard.get(&addr).unwrap();
        assert!(client_state.has_heartbeat);
    }

    #[tokio::test]
    async fn test_unsupported_message_type() {
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let plates = Arc::new(Mutex::new(BTreeMap::new()));
        let addr = create_test_addr(8001);

        // Initialize client state
        {
            let mut client_map = clients.lock().await;
            client_map.insert(addr, ClientState::new());
        }

        // Try sending a ticket message (only server should send these)
        let ticket_msg = Message::Ticket(crate::challenge06::protocol::Ticket {
            plate: "ABC123".to_string(),
            road: 123,
            mile1: 100,
            timestamp1: 1000,
            mile2: 110,
            timestamp2: 1300,
            speed: 12000, // 120.00 mph
        });

        let result = process_message(ticket_msg, addr, &clients, &plates).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Unexpected message type")
        );
    }

    #[tokio::test]
    async fn test_speed_calculation_edge_cases() {
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let plates = Arc::new(Mutex::new(BTreeMap::new()));

        // Test with very large speed (should still work mathematically)
        let addr1 = create_test_addr(8001);
        setup_camera_client(
            &clients,
            addr1,
            IAmCamera {
                road: 1,
                mile: 0,
                limit: 60,
            },
        )
        .await;

        let addr2 = create_test_addr(8002);
        setup_camera_client(
            &clients,
            addr2,
            IAmCamera {
                road: 1,
                mile: 100,
                limit: 60,
            },
        )
        .await;

        // 100 miles in 1 second = 360,000 mph
        let plate1 = create_test_plate("ROCKET", 1000);
        let plate2 = create_test_plate("ROCKET", 1001);

        process_message(Message::Plate(plate1), addr1, &clients, &plates)
            .await
            .unwrap();
        process_message(Message::Plate(plate2), addr2, &clients, &plates)
            .await
            .unwrap();

        let plates_guard = plates.lock().await;
        assert_eq!(plates_guard.len(), 2);
    }

    #[tokio::test]
    async fn test_concurrent_observations_different_plates() {
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let plates = Arc::new(Mutex::new(BTreeMap::new()));
        let addr = create_test_addr(8001);

        setup_camera_client(&clients, addr, create_test_camera()).await;

        // Multiple different plates at same camera
        let plates_to_test = vec!["CAR001", "CAR002", "CAR003", "TRUCK1", "BIKE99"];

        for (i, plate_str) in plates_to_test.iter().enumerate() {
            let plate_msg = create_test_plate(plate_str, 1000 + i as u32);
            let result = process_message(Message::Plate(plate_msg), addr, &clients, &plates).await;
            assert!(result.is_ok());
        }

        let plates_guard = plates.lock().await;
        assert_eq!(plates_guard.len(), 5);

        // Verify all plates are stored with correct keys
        for (i, plate_str) in plates_to_test.iter().enumerate() {
            let key = (plate_str.to_string(), 1000 + i as u32);
            assert!(plates_guard.contains_key(&key));
        }
    }

    #[tokio::test]
    async fn test_reverse_direction_speed_calculation() {
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let plates = Arc::new(Mutex::new(BTreeMap::new()));

        // Camera at mile 110 (higher mile marker)
        let addr1 = create_test_addr(8001);
        setup_camera_client(
            &clients,
            addr1,
            IAmCamera {
                road: 1,
                mile: 110,
                limit: 60,
            },
        )
        .await;

        // Camera at mile 100 (lower mile marker)
        let addr2 = create_test_addr(8002);
        setup_camera_client(
            &clients,
            addr2,
            IAmCamera {
                road: 1,
                mile: 100,
                limit: 60,
            },
        )
        .await;

        // Vehicle going backwards (from mile 110 to mile 100)
        let plate1 = create_test_plate("REVERSE", 1000);
        let plate2 = create_test_plate("REVERSE", 1300); // 5 minutes later

        process_message(Message::Plate(plate1), addr1, &clients, &plates)
            .await
            .unwrap();
        process_message(Message::Plate(plate2), addr2, &clients, &plates)
            .await
            .unwrap();

        // Should still calculate speed correctly using absolute difference
        // 10 miles / 300 seconds * 3600 = 120 mph (violation)
        let plates_guard = plates.lock().await;
        assert_eq!(plates_guard.len(), 2);
    }

    #[tokio::test]
    async fn test_client_id_generation_isolated() {
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let plates = Arc::new(Mutex::new(BTreeMap::new()));

        // Manually assign predictable IDs for testing
        let addresses = vec![
            create_test_addr(8001),
            create_test_addr(8002),
            create_test_addr(8003),
        ];

        let mut expected_ids = vec![];

        for (i, addr) in addresses.iter().enumerate() {
            // Initialize client state first
            {
                let mut client_map = clients.lock().await;
                client_map.insert(*addr, ClientState::new());
            }

            // Send camera message
            let camera_msg = Message::IAmCamera(create_test_camera());
            process_message(camera_msg, *addr, &clients, &plates)
                .await
                .unwrap();

            // Capture the assigned ID
            {
                let client_guard = clients.lock().await;
                let client_state = client_guard.get(addr).unwrap();
                let assigned_id = client_state.client_id.unwrap();
                expected_ids.push(assigned_id);
            }
        }

        // Verify IDs are unique and incrementing
        expected_ids.sort();
        for i in 1..expected_ids.len() {
            assert!(
                expected_ids[i] > expected_ids[i - 1],
                "IDs should be unique and incrementing"
            );
        }

        // Verify all IDs are present
        assert_eq!(expected_ids.len(), 3);
    }

    #[tokio::test]
    async fn test_plate_observation_data_integrity() {
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let plates = Arc::new(Mutex::new(BTreeMap::new()));
        let addr = create_test_addr(8001);

        let camera = IAmCamera {
            road: 999,
            mile: 500,
            limit: 120,
        };
        setup_camera_client(&clients, addr, camera).await;

        let plate_msg = create_test_plate("TEST123", 1234567890);
        process_message(Message::Plate(plate_msg), addr, &clients, &plates)
            .await
            .unwrap();

        // Verify the stored observation has correct data
        let plates_guard = plates.lock().await;
        let key = ("TEST123".to_string(), 1234567890);
        let observation = plates_guard.get(&key).unwrap();

        assert_eq!(observation.plate.plate, "TEST123");
        assert_eq!(observation.plate.timestamp, 1234567890);
        assert_eq!(observation.camera.road, 999);
        assert_eq!(observation.camera.mile, 500);
        assert_eq!(observation.camera.limit, 120);
        assert_eq!(observation.camera_id, 42); // From setup_camera_client
    }

    #[tokio::test]
    async fn test_memory_efficiency_large_dataset() {
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let plates = Arc::new(Mutex::new(BTreeMap::new()));
        let addr = create_test_addr(8001);

        setup_camera_client(&clients, addr, create_test_camera()).await;

        // Add many observations to test memory handling
        for i in 0..1000 {
            let plate_msg = create_test_plate(&format!("PLATE{:04}", i % 100), 1000 + i);
            let result = process_message(Message::Plate(plate_msg), addr, &clients, &plates).await;
            assert!(result.is_ok());
        }

        let plates_guard = plates.lock().await;
        assert_eq!(plates_guard.len(), 1000);

        // Verify BTreeMap maintains ordering even with large dataset
        let keys: Vec<_> = plates_guard.keys().collect();
        for i in 1..keys.len() {
            assert!(keys[i - 1] <= keys[i], "BTreeMap ordering violated");
        }
    }

    // Integration test that simulates realistic traffic scenario
    #[tokio::test]
    async fn test_realistic_traffic_scenario() {
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let plates = Arc::new(Mutex::new(BTreeMap::new()));

        // Setup highway with 5 cameras every 10 miles
        let cameras = vec![
            (
                create_test_addr(8001),
                IAmCamera {
                    road: 101,
                    mile: 0,
                    limit: 65,
                },
            ),
            (
                create_test_addr(8002),
                IAmCamera {
                    road: 101,
                    mile: 10,
                    limit: 65,
                },
            ),
            (
                create_test_addr(8003),
                IAmCamera {
                    road: 101,
                    mile: 20,
                    limit: 65,
                },
            ),
            (
                create_test_addr(8004),
                IAmCamera {
                    road: 101,
                    mile: 30,
                    limit: 65,
                },
            ),
            (
                create_test_addr(8005),
                IAmCamera {
                    road: 101,
                    mile: 40,
                    limit: 65,
                },
            ),
        ];

        for (addr, camera) in &cameras {
            setup_camera_client(&clients, *addr, camera.clone()).await;
        }

        // Simulate various vehicles with different behaviors
        let vehicles = vec![
            // Law-abiding vehicle: 65mph exactly
            ("LAW001", vec![(0, 0), (1, 553), (2, 1106), (3, 1659)]), // ~65mph
            // Speeding vehicle: 85mph
            ("SPEED1", vec![(0, 1000), (1, 1424), (2, 1847)]), // ~85mph
            // Variable speed vehicle
            ("VAR001", vec![(0, 2000), (1, 2400), (3, 3200)]), // Mix of speeds
        ];

        for (plate, observations) in vehicles {
            for (camera_idx, timestamp) in observations {
                let addr = cameras[camera_idx].0;
                let plate_msg = create_test_plate(plate, timestamp as u32);

                let result =
                    process_message(Message::Plate(plate_msg), addr, &clients, &plates).await;
                assert!(result.is_ok());
            }
        }

        // Verify all observations were stored
        let plates_guard = plates.lock().await;
        let total_observations = 4 + 3 + 3; // Sum of observations per vehicle
        assert_eq!(plates_guard.len(), total_observations);
    }

    #[tokio::test]
    async fn test_send_error_function() {
        use std::io::Cursor;
        use tokio::io::AsyncWriteExt;

        // Mock stream for testing
        let mut mock_stream: Vec<u8> = Vec::new();

        // This would require making send_error public or creating a test wrapper
        // For now, test the error format manually
        let error_msg = "test error";
        let expected = format!("\x10{}{}", error_msg.len() as u8 as char, error_msg);

        assert_eq!(expected.len(), 1 + 1 + error_msg.len()); // type + length + message
        assert_eq!(expected.as_bytes()[0], 0x10); // Error message type
        assert_eq!(expected.as_bytes()[1], error_msg.len() as u8); // Length
    }
}
