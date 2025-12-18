"""Casparian Flow plugin for parsing aviation/defense system telemetry CSV data."""

from io import StringIO
from datetime import datetime
import pandas as pd
from casparian_flow.sdk import FileEvent, Plugin, PluginMetadata

MANIFEST = PluginMetadata(subscriptions=["raw_mcdata_events"])


class MCDataParser(Plugin):
    """Parse heterogeneous aviation/defense telemetry CSV into multiple tables."""

    def consume(self, event: FileEvent):
        """Parse CSV file and route rows to appropriate tables based on record_type."""
        with open(event.path, 'r', encoding='utf-8') as f:
            lines = f.readlines()

        # Storage for each table type
        software_rows = []
        hardware_rows = []
        firmware_rows = []
        config_faults_rows = []
        ofp_rows = []
        mdl_rows = []
        test_states_rows = []

        for line in lines:
            parts = [p.strip() for p in line.strip().split(',')]
            if len(parts) < 3:
                continue

            record_id = int(parts[0]) if parts[0] else None
            record_type = parts[1]
            
            # Route based on record_type pattern
            if '_SW' in record_type:
                # Active Component Software
                software_rows.append({
                    'record_id': record_id,
                    'record_type': record_type,
                    'component_name': record_type.replace('ACT_', '').replace('_SW', ''),
                    'field_3': parts[2] if len(parts) > 2 else '',
                    'timestamp': self._parse_timestamp(parts[3]) if len(parts) > 3 else None,
                    'field_5': parts[4] if len(parts) > 4 else '',
                    'field_6': parts[5] if len(parts) > 5 else '',
                    'sw_metric_1': int(parts[6]) if len(parts) > 6 and parts[6] else 0,
                    'sw_metric_2': int(parts[7]) if len(parts) > 7 and parts[7] else 0,
                    'sw_metric_3': int(parts[8]) if len(parts) > 8 and parts[8] else 0,
                    'sw_metric_4': int(parts[9]) if len(parts) > 9 and parts[9] else 0,
                    'additional_data': ','.join(parts[10:]) if len(parts) > 10 else ''
                })
            elif '_HW' in record_type:
                # Active Component Hardware
                hardware_rows.append({
                    'record_id': record_id,
                    'record_type': record_type,
                    'component_name': record_type.replace('ACT_', '').replace('_HW', ''),
                    'field_3': parts[2] if len(parts) > 2 else '',
                    'timestamp': self._parse_timestamp(parts[3]) if len(parts) > 3 else None,
                    'field_5': parts[4] if len(parts) > 4 else '',
                    'field_6': parts[5] if len(parts) > 5 else '',
                    'hw_metric_1': int(parts[6]) if len(parts) > 6 and parts[6] else 0,
                    'hw_metric_2': int(parts[7]) if len(parts) > 7 and parts[7] else 0,
                    'hw_metric_3': int(parts[8]) if len(parts) > 8 and parts[8] else 0,
                    'hw_metric_4': int(parts[9]) if len(parts) > 9 and parts[9] else 0,
                    'hw_metric_5': int(parts[10]) if len(parts) > 10 and parts[10] else 0,
                    'hw_metric_6': int(parts[11]) if len(parts) > 11 and parts[11] else 0,
                    'hw_metric_7': int(parts[12]) if len(parts) > 12 and parts[12] else 0,
                    'hw_metric_8': int(parts[13]) if len(parts) > 13 and parts[13] else 0,
                    'hw_metric_9': int(parts[14]) if len(parts) > 14 and parts[14] else 0,
                    'hw_metric_10': int(parts[15]) if len(parts) > 15 and parts[15] else 0,
                    'hw_metric_11': int(parts[16]) if len(parts) > 16 and parts[16] else 0,
                    'hw_metric_12': int(parts[17]) if len(parts) > 17 and parts[17] else 0
                })
            elif '_FW' in record_type:
                # Active Component Firmware
                firmware_rows.append({
                    'record_id': record_id,
                    'record_type': record_type,
                    'component_name': record_type.replace('ACT_', '').replace('_FW', ''),
                    'field_3': parts[2] if len(parts) > 2 else '',
                    'timestamp': self._parse_timestamp(parts[3]) if len(parts) > 3 else None,
                    'field_5': parts[4] if len(parts) > 4 else '',
                    'field_6': parts[5] if len(parts) > 5 else '',
                    'fw_count': int(parts[6]) if len(parts) > 6 and parts[6] else 0,
                    'additional_data': ','.join(parts[7:]) if len(parts) > 7 else ''
                })
            elif 'CONFIG_FLTS' in record_type:
                # Configuration Faults
                config_faults_rows.append({
                    'record_id': record_id,
                    'record_type': record_type,
                    'field_3': parts[2] if len(parts) > 2 else '',
                    'timestamp': self._parse_timestamp(parts[3]) if len(parts) > 3 else None,
                    'component_name': parts[4] if len(parts) > 4 else '',
                    'fault_category': parts[5] if len(parts) > 5 else '',
                    'fault_status': parts[6] if len(parts) > 6 else ''
                })
            elif '_OFP' in record_type:
                # Operational Flight Program
                ofp_rows.append({
                    'record_id': record_id,
                    'record_type': record_type,
                    'component_name': record_type.replace('ACT_', '').replace('_OFP', ''),
                    'field_3': parts[2] if len(parts) > 2 else '',
                    'timestamp': self._parse_timestamp(parts[3]) if len(parts) > 3 else None,
                    'field_5': parts[4] if len(parts) > 4 else '',
                    'field_6': parts[5] if len(parts) > 5 else '',
                    'version': parts[6] if len(parts) > 6 else ''
                })
            elif '_MDL' in record_type:
                # Mission Data Load
                mdl_rows.append({
                    'record_id': record_id,
                    'record_type': record_type,
                    'field_3': parts[2] if len(parts) > 2 else '',
                    'timestamp': self._parse_timestamp(parts[3]) if len(parts) > 3 else None,
                    'field_5': parts[4] if len(parts) > 4 else '',
                    'field_6': parts[5] if len(parts) > 5 else '',
                    'classified_constants': parts[6] if len(parts) > 6 else '',
                    'field_8': parts[7] if len(parts) > 7 else '',
                    'mission_parameters': parts[8] if len(parts) > 8 else '',
                    'field_10': parts[9] if len(parts) > 9 else '',
                    'calibration_data': parts[10] if len(parts) > 10 else '',
                    'field_12': parts[11] if len(parts) > 11 else '',
                    'scan_tables': parts[12] if len(parts) > 12 else '',
                    'field_14': parts[13] if len(parts) > 13 else '',
                    'obt_table': parts[14] if len(parts) > 14 else '',
                    'field_16': parts[15] if len(parts) > 15 else '',
                    'eid_library': parts[16] if len(parts) > 16 else '',
                    'field_18': parts[17] if len(parts) > 17 else '',
                    'ecn_bst_mapping': parts[18] if len(parts) > 18 else ''
                })
            elif 'TEST_STATES' in record_type:
                # Test States
                test_states_rows.append({
                    'record_id': record_id,
                    'record_type': record_type,
                    'component_name': record_type.replace('TEST_STATES_', ''),
                    'field_3': parts[2] if len(parts) > 2 else '',
                    'ibit_indicator': parts[3] if len(parts) > 3 else '',
                    'ibit_status': parts[4] if len(parts) > 4 else '',
                    'timeout_faults_indicator': parts[5] if len(parts) > 5 else '',
                    'ibit_test_type': parts[6] if len(parts) > 6 else '',
                    'ibit_duration': parts[7] if len(parts) > 7 else '',
                    'ibit_result': parts[8] if len(parts) > 8 else '',
                    'pbit_test_type': parts[9] if len(parts) > 9 else '',
                    'pbit_duration': parts[10] if len(parts) > 10 else '',
                    'pbit_result': parts[11] if len(parts) > 11 else '',
                    'sbit_test_type': parts[12] if len(parts) > 12 else '',
                    'sbit_duration': parts[13] if len(parts) > 13 else '',
                    'sbit_result': parts[14] if len(parts) > 14 else ''
                })

        # Publish each table
        if software_rows:
            self.publish('active_component_software', pd.DataFrame(software_rows))
        if hardware_rows:
            self.publish('active_component_hardware', pd.DataFrame(hardware_rows))
        if firmware_rows:
            self.publish('active_component_firmware', pd.DataFrame(firmware_rows))
        if config_faults_rows:
            self.publish('config_faults', pd.DataFrame(config_faults_rows))
        if ofp_rows:
            self.publish('active_component_ofp', pd.DataFrame(ofp_rows))
        if mdl_rows:
            self.publish('active_component_mdl', pd.DataFrame(mdl_rows))
        if test_states_rows:
            self.publish('test_states', pd.DataFrame(test_states_rows))

    def _parse_timestamp(self, ts_str):
        """Parse timestamp string to datetime."""
        if not ts_str:
            return None
        try:
            return datetime.fromisoformat(ts_str.replace('Z', '+00:00'))
        except:
            return None