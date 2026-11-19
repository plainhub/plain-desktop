#import <CoreBluetooth/CoreBluetooth.h>

typedef void (*PlainBleState)(NSInteger);
typedef bool (*PlainBleAdvertisement)(const char *, const char *);
typedef void (*PlainBleFrame)(const void *, size_t);
typedef void (*PlainBleCompletion)(uint64_t, bool);

@interface PlainBleRadio : NSObject <CBCentralManagerDelegate, CBPeripheralDelegate>
@property(nonatomic) CBCentralManager *manager;
@property(nonatomic) CBPeripheral *peer;
@property(nonatomic) CBUUID *service;
@property(nonatomic) CBUUID *target;
@property(nonatomic) NSMutableDictionary<NSUUID *, CBPeripheral *> *discovered;
@property(nonatomic) CBCharacteristic *characteristic;
@property(nonatomic) BOOL scanning;
@property(nonatomic) uint64_t connecting;
@property(nonatomic) uint64_t writing;
@property(nonatomic) PlainBleState stateCallback;
@property(nonatomic) PlainBleAdvertisement advertisementCallback;
@property(nonatomic) PlainBleFrame frameCallback;
@property(nonatomic) PlainBleCompletion completion;
@end

static PlainBleRadio *radio;

@implementation PlainBleRadio
- (void)closePeer {
    CBPeripheral *peer = self.peer;
    self.peer = nil;
    self.characteristic = nil;
    uint64_t connecting = self.connecting;
    uint64_t writing = self.writing;
    self.connecting = 0;
    self.writing = 0;
    peer.delegate = nil;
    if (peer) [self.manager cancelPeripheralConnection:peer];
    if (connecting) self.completion(connecting, false);
    if (writing) self.completion(writing, false);
}

- (void)centralManagerDidUpdateState:(CBCentralManager *)central {
    if (central.state == CBManagerStatePoweredOn && self.scanning) {
        [central scanForPeripheralsWithServices:nil options:@{CBCentralManagerScanOptionAllowDuplicatesKey: @YES}];
    } else {
        [self closePeer];
        [self.discovered removeAllObjects];
    }
    if (self.scanning) self.stateCallback(central.state);
}

- (void)centralManager:(CBCentralManager *)central didDiscoverPeripheral:(CBPeripheral *)peer
     advertisementData:(NSDictionary<NSString *, id> *)data RSSI:(NSNumber *)rssi {
    if (!self.scanning) return;
    NSString *identifier = peer.identifier.UUIDString;
    for (NSString *key in @[CBAdvertisementDataServiceUUIDsKey, CBAdvertisementDataOverflowServiceUUIDsKey]) {
        for (CBUUID *service in data[key]) {
            if (self.discovered.count >= 512 && !self.discovered[peer.identifier]) return;
            if (self.advertisementCallback(identifier.UTF8String, service.UUIDString.UTF8String)) {
                self.discovered[peer.identifier] = peer;
            }
        }
    }
}

- (void)centralManager:(CBCentralManager *)central didConnectPeripheral:(CBPeripheral *)peer {
    if (peer != self.peer) return;
    [peer discoverServices:@[self.service]];
}

- (void)centralManager:(CBCentralManager *)central didFailToConnectPeripheral:(CBPeripheral *)peer error:(NSError *)error {
    if (peer == self.peer) [self closePeer];
}

- (void)centralManager:(CBCentralManager *)central didDisconnectPeripheral:(CBPeripheral *)peer error:(NSError *)error {
    if (peer == self.peer) {
        [self closePeer];
        self.frameCallback(NULL, 0);
    }
}

- (void)peripheral:(CBPeripheral *)peer didDiscoverServices:(NSError *)error {
    if (peer != self.peer) return;
    if (!error) for (CBService *service in peer.services) {
        if ([service.UUID isEqual:self.service]) {
            [peer discoverCharacteristics:@[self.target] forService:service];
            return;
        }
    }
    [self closePeer];
}

- (void)peripheral:(CBPeripheral *)peer didDiscoverCharacteristicsForService:(CBService *)service error:(NSError *)error {
    if (peer != self.peer) return;
    if (!error) for (CBCharacteristic *characteristic in service.characteristics) {
        if ([characteristic.UUID isEqual:self.target]) {
            self.characteristic = characteristic;
            [peer setNotifyValue:YES forCharacteristic:characteristic];
            return;
        }
    }
    [self closePeer];
}

- (void)peripheral:(CBPeripheral *)peer didUpdateNotificationStateForCharacteristic:(CBCharacteristic *)characteristic error:(NSError *)error {
    if (peer != self.peer || characteristic != self.characteristic || !self.connecting) return;
    if (error || !characteristic.isNotifying) {
        [self closePeer];
        return;
    }
    uint64_t operation = self.connecting;
    self.connecting = 0;
    self.completion(operation, true);
}

- (void)peripheral:(CBPeripheral *)peer didWriteValueForCharacteristic:(CBCharacteristic *)characteristic error:(NSError *)error {
    if (peer != self.peer || characteristic != self.characteristic || !self.writing) return;
    uint64_t operation = self.writing;
    self.writing = 0;
    self.completion(operation, !error);
}

- (void)peripheral:(CBPeripheral *)peer didUpdateValueForCharacteristic:(CBCharacteristic *)characteristic error:(NSError *)error {
    if (peer == self.peer && characteristic == self.characteristic && !error && characteristic.value.length) {
        self.frameCallback(characteristic.value.bytes, characteristic.value.length);
    }
}
@end

void plain_macos_ble_start(PlainBleState state, PlainBleAdvertisement advertisement,
                          PlainBleFrame frame, PlainBleCompletion completion) {
    if (!radio) {
        radio = [PlainBleRadio new];
        radio.discovered = [NSMutableDictionary dictionary];
        radio.stateCallback = state;
        radio.advertisementCallback = advertisement;
        radio.frameCallback = frame;
        radio.completion = completion;
    }
    radio.scanning = YES;
    if (!radio.manager) {
        radio.manager = [[CBCentralManager alloc] initWithDelegate:radio queue:nil
            options:@{CBCentralManagerOptionShowPowerAlertKey: @NO}];
    } else {
        [radio centralManagerDidUpdateState:radio.manager];
    }
}

void plain_macos_ble_stop(void) {
    radio.scanning = NO;
    [radio.manager stopScan];
    [radio closePeer];
    [radio.discovered removeAllObjects];
}

void plain_macos_ble_connect(uint64_t operation, const char *identifier, const char *service, const char *target) {
    NSUUID *uuid = [[NSUUID alloc] initWithUUIDString:@(identifier)];
    if (!uuid || !radio.scanning || radio.manager.state != CBManagerStatePoweredOn || radio.peer) {
        radio.completion(operation, false);
        return;
    }
    CBPeripheral *peer = radio.discovered[uuid];
    if (!peer) {
        radio.completion(operation, false);
        return;
    }
    radio.peer = peer;
    radio.connecting = operation;
    radio.service = [CBUUID UUIDWithString:@(service)];
    radio.target = [CBUUID UUIDWithString:@(target)];
    peer.delegate = radio;
    [radio.manager connectPeripheral:peer options:nil];
}

void plain_macos_ble_write(uint64_t operation, const void *bytes, size_t size) {
    if (!radio.characteristic || radio.writing || size > [radio.peer maximumWriteValueLengthForType:CBCharacteristicWriteWithResponse]) {
        radio.completion(operation, false);
        return;
    }
    radio.writing = operation;
    [radio.peer writeValue:[NSData dataWithBytes:bytes length:size] forCharacteristic:radio.characteristic
                      type:CBCharacteristicWriteWithResponse];
}

void plain_macos_ble_disconnect(void) {
    [radio closePeer];
}
