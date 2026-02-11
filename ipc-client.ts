#!/usr/bin/env bun
import * as net from 'net';
import * as fs from 'fs/promises';
import { Logger } from 'tslog';

interface IpcRequest {
    message: string;
    correlation_id?: string;
    socket_type?: 'buy' | 'sell';
}

interface IpcResponse {
    success: boolean;
    token_address?: string | null;
    error?: string;
    correlation_id?: string;
    action?: string; // 'buy' | 'sell' | 'status'
    trade_info?: any;
}

interface TestResult {
    name: string;
    elapsed: number;
    success: boolean;
    hasTokenAddress: boolean;
    hasError: boolean;
    correlationId?: string;
    response?: IpcResponse;
    error?: Error;
    passed: boolean;
    expected?: string | null;
    actual?: string | null;
    socketType?: 'buy' | 'sell' | 'default';
}

class SPLTokenIPCClient {
    private socketPath: string;
    private buySocketPath: string;
    private sellSocketPath: string;
    private clientId: string;
    private logger: Logger<unknown>;
    private currentSocketType: 'default' | 'buy' | 'sell' = 'default';

    constructor(
        defaultSocketPath: string = '/tmp/spl_token_ipc.sock',
        buySocketPath: string = '/tmp/spl_buy.sock',
        sellSocketPath: string = '/tmp/spl_sell.sock'
    ) {
        this.socketPath = defaultSocketPath;
        this.buySocketPath = buySocketPath;
        this.sellSocketPath = sellSocketPath;
        this.clientId = `client-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
        
        this.logger = new Logger({
            name: 'SPLTokenIPCClient',
            type: 'pretty',
            minLevel: 'info',
            displayFunctionName: false,
            displayFilePath: 'hidden',
            displayInstanceName: true,
            instanceName: this.clientId,
        });
    }

    setSocketType(socketType: 'default' | 'buy' | 'sell'): void {
        const oldType = this.currentSocketType;
        const oldPath = this.socketPath;
        
        this.currentSocketType = socketType;
        
        switch (socketType) {
            case 'buy':
                this.socketPath = this.buySocketPath;
                break;
            case 'sell':
                this.socketPath = this.sellSocketPath;
                break;
            case 'default':
                this.socketPath = '/tmp/spl_token_ipc.sock';
                break;
        }
        
        this.logger.info('Socket type changed', {
            oldType,
            newType: socketType,
            oldPath,
            newPath: this.socketPath
        });
    }

    getCurrentSocketInfo(): { type: string; path: string } {
        return {
            type: this.currentSocketType,
            path: this.socketPath
        };
    }

    private getSocketPathForType(socketType?: 'buy' | 'sell'): string {
        if (socketType === 'buy') {
            return this.buySocketPath;
        } else if (socketType === 'sell') {
            return this.sellSocketPath;
        }
        return this.socketPath;
    }

    private async connect(socketType?: 'buy' | 'sell'): Promise<net.Socket> {
        const actualSocketPath = this.getSocketPathForType(socketType);
        
        return new Promise((resolve, reject) => {
            const socket = net.createConnection({ path: actualSocketPath }, () => {
                this.logger.debug('Connected to IPC socket', {
                    socketPath: actualSocketPath,
                    socketType: socketType || this.currentSocketType
                });
                resolve(socket);
            });

            socket.on('error', (error) => {
                this.logger.error('Socket connection error', error, {
                    socketPath: actualSocketPath,
                    socketType: socketType || this.currentSocketType
                });
                reject(error);
            });
        });
    }

    async sendMessage(message: string, socketType?: 'buy' | 'sell'): Promise<IpcResponse> {
        const correlationId = `${this.clientId}-${Date.now()}`;
        const actualSocketType = socketType || this.currentSocketType;
        
        this.logger.debug('Sending message', {
            correlationId,
            socketType: actualSocketType,
            socketPath: this.getSocketPathForType(socketType),
            messageLength: message.length,
            messagePreview: message.length > 100 ? message.substring(0, 100) + '...' : message
        });

        const socket = await this.connect(socketType);
        
        return new Promise((resolve, reject) => {
            const request: IpcRequest = {
                message,
                correlation_id: correlationId,
                socket_type: socketType
            };

            const requestJson = JSON.stringify(request) + '\n';
            
            let responseData = '';
            
            socket.on('data', (data) => {
                responseData += data.toString();
                
                if (responseData.includes('\n')) {
                    try {
                        const response: IpcResponse = JSON.parse(responseData.trim());
                        this.logger.debug('Received response', {
                            correlationId,
                            socketType: actualSocketType,
                            success: response.success,
                            hasTokenAddress: !!response.token_address,
                            hasError: !!response.error,
                            action: response.action
                        });
                        
                        socket.end();
                        resolve(response);
                    } catch (error) {
                        this.logger.error('Failed to parse response', error, {
                            socketType: actualSocketType,
                            rawResponse: responseData.trim()
                        });
                        socket.end();
                        reject(new Error(`Failed to parse response: ${error}`));
                    }
                }
            });

            socket.on('error', (error) => {
                this.logger.error('Socket error during communication', error, {
                    correlationId,
                    socketType: actualSocketType
                });
                socket.end();
                reject(error);
            });

            socket.on('timeout', () => {
                this.logger.error('Socket timeout', {
                    correlationId,
                    socketType: actualSocketType,
                    timeout: 5000
                });
                socket.end();
                reject(new Error('Socket timeout'));
            });

            socket.on('end', () => {
                this.logger.trace('Socket connection ended', {
                    correlationId,
                    socketType: actualSocketType
                });
            });

            socket.setTimeout(5000);
            // socket.write(requestJson);
            socket.write(message);
        });
    }

    // New method to test both buy and sell sockets
    async testBuySellSocket(mintAddress: string, config?: any): Promise<TestResult[]> {
        this.logger.info('Testing buy and sell sockets', { mintAddress });
        
        const results: TestResult[] = [];
        
        // Test buy socket
        try {
            const buyStartTime = Date.now();
            let buyMessage = mintAddress;
            
            if (config) {
                buyMessage = JSON.stringify({
                    mint: mintAddress,
                    ...config
                });
            }
            
            const buyResponse = await this.sendMessage(buyMessage, 'buy');
            const buyElapsed = Date.now() - buyStartTime;
            
            results.push({
                name: `Buy-${mintAddress.substring(0, 8)}...`,
                elapsed: buyElapsed,
                success: buyResponse.success,
                hasTokenAddress: !!buyResponse.token_address,
                hasError: !!buyResponse.error,
                correlationId: buyResponse.correlation_id,
                response: buyResponse,
                passed: buyResponse.success,
                socketType: 'buy'
            });
            
            this.logger.info('Buy socket test completed', {
                success: buyResponse.success,
                elapsed: buyElapsed,
                action: buyResponse.action
            });
            
            // Wait a bit before testing sell socket
            await new Promise(resolve => setTimeout(resolve, 2000));
            
        } catch (error) {
            this.logger.error('Buy socket test failed', error);
            results.push({
                name: `Buy-${mintAddress.substring(0, 8)}...`,
                elapsed: 0,
                success: false,
                hasTokenAddress: false,
                hasError: true,
                error: error as Error,
                passed: false,
                socketType: 'buy'
            });
        }
        
        // Test sell socket
        try {
            const sellStartTime = Date.now();
            let sellMessage = mintAddress;
            
            if (config) {
                sellMessage = JSON.stringify({
                    mint: mintAddress,
                    ...config,
                    forceSell: true
                });
            }
            
            const sellResponse = await this.sendMessage(sellMessage, 'sell');
            const sellElapsed = Date.now() - sellStartTime;
            
            results.push({
                name: `Sell-${mintAddress.substring(0, 8)}...`,
                elapsed: sellElapsed,
                success: sellResponse.success,
                hasTokenAddress: !!sellResponse.token_address,
                hasError: !!sellResponse.error,
                correlationId: sellResponse.correlation_id,
                response: sellResponse,
                passed: sellResponse.success,
                socketType: 'sell'
            });
            
            this.logger.info('Sell socket test completed', {
                success: sellResponse.success,
                elapsed: sellElapsed,
                action: sellResponse.action
            });
            
        } catch (error) {
            this.logger.error('Sell socket test failed', error);
            results.push({
                name: `Sell-${mintAddress.substring(0, 8)}...`,
                elapsed: 0,
                success: false,
                hasTokenAddress: false,
                hasError: true,
                error: error as Error,
                passed: false,
                socketType: 'sell'
            });
        }
        
        return results;
    }

    // Test sequential buy then sell
    async testBuyThenSellSequence(mintAddress: string, buyConfig?: any, sellConfig?: any): Promise<TestResult[]> {
        this.logger.info('Testing buy then sell sequence', { mintAddress });
        
        const results: TestResult[] = [];
        const sequenceId = `seq-${Date.now()}`;
        
        // Step 1: Buy token
        try {
            this.logger.info('Step 1: Buying token', { mintAddress, sequenceId });
            
            const buyStartTime = Date.now();
            let buyMessage = mintAddress;
            
            if (buyConfig) {
                buyMessage = JSON.stringify({
                    mint: mintAddress,
                    ...buyConfig,
                    sequenceId
                });
            }
            
            const buyResponse = await this.sendMessage(buyMessage, 'buy');
            const buyElapsed = Date.now() - buyStartTime;
            
            results.push({
                name: `${sequenceId}-Buy`,
                elapsed: buyElapsed,
                success: buyResponse.success,
                hasTokenAddress: !!buyResponse.token_address,
                hasError: !!buyResponse.error,
                correlationId: buyResponse.correlation_id,
                response: buyResponse,
                passed: buyResponse.success,
                socketType: 'buy'
            });
            
            if (!buyResponse.success) {
                this.logger.error('Buy step failed, skipping sell', {
                    sequenceId,
                    error: buyResponse.error
                });
                return results;
            }
            
            // Wait for token to be cached (adjust based on your server)
            this.logger.info('Waiting for token to be cached...', { sequenceId });
            await new Promise(resolve => setTimeout(resolve, 3000));
            
        } catch (error) {
            this.logger.error('Buy step failed', error, { sequenceId });
            results.push({
                name: `${sequenceId}-Buy`,
                elapsed: 0,
                success: false,
                hasTokenAddress: false,
                hasError: true,
                error: error as Error,
                passed: false,
                socketType: 'buy'
            });
            return results;
        }
        
        // Step 2: Sell token
        try {
            this.logger.info('Step 2: Selling token', { mintAddress, sequenceId });
            
            const sellStartTime = Date.now();
            let sellMessage = mintAddress;
            
            if (sellConfig) {
                sellMessage = JSON.stringify({
                    mint: mintAddress,
                    ...sellConfig,
                    sequenceId,
                    forceSell: true
                });
            }
            
            const sellResponse = await this.sendMessage(sellMessage, 'sell');
            const sellElapsed = Date.now() - sellStartTime;
            
            results.push({
                name: `${sequenceId}-Sell`,
                elapsed: sellElapsed,
                success: sellResponse.success,
                hasTokenAddress: !!sellResponse.token_address,
                hasError: !!sellResponse.error,
                correlationId: sellResponse.correlation_id,
                response: sellResponse,
                passed: sellResponse.success,
                socketType: 'sell'
            });
            
        } catch (error) {
            this.logger.error('Sell step failed', error, { sequenceId });
            results.push({
                name: `${sequenceId}-Sell`,
                elapsed: 0,
                success: false,
                hasTokenAddress: false,
                hasError: true,
                error: error as Error,
                passed: false,
                socketType: 'sell'
            });
        }
        
        return results;
    }

    // Get server status from both sockets
    async getServerStatus(): Promise<{
        buyStatus?: IpcResponse;
        sellStatus?: IpcResponse;
        defaultStatus?: IpcResponse;
    }> {
        this.logger.info('Getting server status from all sockets');
        
        const status = {};
        
        // Try default socket
        try {
            const defaultResponse = await this.sendMessage('status');
            status.defaultStatus = defaultResponse;
        } catch (error) {
            this.logger.warn('Default socket status failed', error);
        }
        
        // Try buy socket
        try {
            const buyResponse = await this.sendMessage('status', 'buy');
            status.buyStatus = buyResponse;
        } catch (error) {
            this.logger.warn('Buy socket status failed', error);
        }
        
        // Try sell socket
        try {
            const sellResponse = await this.sendMessage('status', 'sell');
            status.sellStatus = sellResponse;
        } catch (error) {
            this.logger.warn('Sell socket status failed', error);
        }
        
        return status;
    }

    // Test socket switching functionality
    async testSocketSwitching(): Promise<TestResult[]> {
        this.logger.info('Testing socket switching functionality');
        
        const results: TestResult[] = [];
        const testMessages = [
            'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v',
            'So11111111111111111111111111111111111111112',
            'DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263'
        ];
        
        for (const [index, message] of testMessages.entries()) {
            // Test with explicit socket type parameter
            try {
                const startTime = Date.now();
                const buyResponse = await this.sendMessage(message, 'buy');
                const elapsed = Date.now() - startTime;
                
                results.push({
                    name: `Explicit-Buy-${index}`,
                    elapsed,
                    success: buyResponse.success,
                    hasTokenAddress: !!buyResponse.token_address,
                    hasError: !!buyResponse.error,
                    correlationId: buyResponse.correlation_id,
                    response: buyResponse,
                    passed: true,
                    socketType: 'buy'
                });
            } catch (error) {
                results.push({
                    name: `Explicit-Buy-${index}`,
                    elapsed: 0,
                    success: false,
                    hasTokenAddress: false,
                    hasError: true,
                    error: error as Error,
                    passed: false,
                    socketType: 'buy'
                });
            }
            
            // Switch socket and test
            this.setSocketType('sell');
            
            try {
                const startTime = Date.now();
                const sellResponse = await this.sendMessage(message);
                const elapsed = Date.now() - startTime;
                
                results.push({
                    name: `Switched-Sell-${index}`,
                    elapsed,
                    success: sellResponse.success,
                    hasTokenAddress: !!sellResponse.token_address,
                    hasError: !!sellResponse.error,
                    correlationId: sellResponse.correlation_id,
                    response: sellResponse,
                    passed: true,
                    socketType: 'sell'
                });
            } catch (error) {
                results.push({
                    name: `Switched-Sell-${index}`,
                    elapsed: 0,
                    success: false,
                    hasTokenAddress: false,
                    hasError: true,
                    error: error as Error,
                    passed: false,
                    socketType: 'sell'
                });
            }
            
            // Switch back to default
            this.setSocketType('default');
            
            await new Promise(resolve => setTimeout(resolve, 500));
        }
        
        return results;
    }

    async sendMessageAsync(message: string, correlationId?: string, socketType?: 'buy' | 'sell'): Promise<IpcResponse> {
        const actualCorrelationId = correlationId || `${this.clientId}-${Date.now()}-${Math.random().toString(36).substr(2, 6)}`;
        const actualSocketType = socketType || this.currentSocketType;
        
        this.logger.debug('Sending message asynchronously', {
            correlationId: actualCorrelationId,
            socketType: actualSocketType,
            messageLength: message.length
        });

        try {
            const response = await this.sendMessage(message, socketType);
            response.correlation_id = actualCorrelationId;
            return response;
        } catch (error) {
            this.logger.error('Async message failed', error, {
                correlationId: actualCorrelationId,
                socketType: actualSocketType
            });
            throw error;
        }
    }

    async sendMessagesConcurrently(messages: Array<{message: string; name?: string; socketType?: 'buy' | 'sell'}>): Promise<TestResult[]> {
        this.logger.info('Starting concurrent message sending', {
            messageCount: messages.length,
            socketTypes: messages.map(m => m.socketType || 'default'),
            concurrencyLimit: 10
        });

        const results: TestResult[] = [];
        const concurrencyLimit = 10;
        const batches = [];

        // Split messages into batches
        for (let i = 0; i < messages.length; i += concurrencyLimit) {
            batches.push(messages.slice(i, i + concurrencyLimit));
        }

        for (const [batchIndex, batch] of batches.entries()) {
            this.logger.info(`Processing batch ${batchIndex + 1}/${batches.length}`, {
                batchSize: batch.length,
                totalMessages: messages.length
            });

            const batchPromises = batch.map(async (item, indexInBatch) => {
                const globalIndex = batchIndex * concurrencyLimit + indexInBatch;
                const testName = item.name || `Message-${globalIndex + 1}`;
                const socketType = item.socketType;
                
                const testLogger = this.logger.getChildLogger({
                    instanceName: `Test-${globalIndex + 1}`
                });

                testLogger.debug('Processing message in batch', {
                    testName,
                    socketType,
                    batchIndex: batchIndex + 1,
                    indexInBatch,
                    messageLength: item.message.length
                });

                const startTime = Date.now();
                
                try {
                    const response = await this.sendMessageAsync(item.message, undefined, socketType);
                    const elapsed = Date.now() - startTime;

                    const result: TestResult = {
                        name: testName,
                        elapsed,
                        success: response.success,
                        hasTokenAddress: !!response.token_address,
                        hasError: !!response.error,
                        correlationId: response.correlation_id,
                        response,
                        passed: true,
                        actual: response.token_address || null,
                        socketType: socketType || this.currentSocketType
                    };

                    testLogger.debug('Message processed successfully', {
                        testName,
                        socketType,
                        elapsed,
                        success: response.success
                    });

                    return result;
                } catch (error) {
                    const elapsed = Date.now() - startTime;
                    const result: TestResult = {
                        name: testName,
                        elapsed,
                        success: false,
                        hasTokenAddress: false,
                        hasError: true,
                        error: error as Error,
                        passed: false,
                        socketType: socketType || this.currentSocketType
                    };

                    testLogger.error('Message processing failed', error, {
                        testName,
                        socketType,
                        elapsed
                    });

                    return result;
                }
            });

            // Process batch concurrently
            const batchResults = await Promise.all(batchPromises);
            results.push(...batchResults);

            // Small delay between batches if needed
            if (batchIndex < batches.length - 1) {
                await new Promise(resolve => setTimeout(resolve, 100));
            }
        }

        this.logger.info('All messages processed', {
            totalProcessed: results.length,
            successful: results.filter(r => r.success).length,
            failed: results.filter(r => !r.success).length,
            bySocketType: {
                buy: results.filter(r => r.socketType === 'buy').length,
                sell: results.filter(r => r.socketType === 'sell').length,
                default: results.filter(r => r.socketType === 'default').length
            }
        });

        return results;
    }

    async testMultipleMessagesConcurrently(): Promise<void> {
        const testCases = [
            {
                name: "USDC-Buy",
                message: 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v',
                socketType: 'buy' as const,
                expected: 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v'
            },
            {
                name: "USDC-Sell",
                message: 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v',
                socketType: 'sell' as const,
                expected: 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v'
            },
            {
                name: "SOL-Buy",
                message: 'So11111111111111111111111111111111111111112',
                socketType: 'buy' as const,
                expected: 'So11111111111111111111111111111111111111112'
            },
            {
                name: "SOL-Sell",
                message: 'So11111111111111111111111111111111111111112',
                socketType: 'sell' as const,
                expected: 'So11111111111111111111111111111111111111112'
            },
            {
                name: "Default-Socket",
                message: 'DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263',
                socketType: 'default' as const,
                expected: 'DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263'
            },
            {
                name: "Mixed-Buy",
                message: 'Send to EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v please',
                socketType: 'buy' as const,
                expected: 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v'
            },
            {
                name: "JSON-Buy",
                message: JSON.stringify({
                    mint: 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v',
                    amount: 1000000,
                    percentage: 0.5
                }),
                socketType: 'buy' as const,
                expected: 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v'
            },
            {
                name: "JSON-Sell",
                message: JSON.stringify({
                    mint: 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v',
                    percentage: 1.0,
                    forceSell: true
                }),
                socketType: 'sell' as const,
                expected: 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v'
            }
        ];

        this.logger.info('Starting concurrent test suite with multiple sockets', {
            testCount: testCases.length,
            buyTests: testCases.filter(tc => tc.socketType === 'buy').length,
            sellTests: testCases.filter(tc => tc.socketType === 'sell').length,
            defaultTests: testCases.filter(tc => tc.socketType === 'default').length
        });

        const startTime = Date.now();
        const results = await this.sendMessagesConcurrently(testCases);
        const totalElapsed = Date.now() - startTime;

        // Calculate statistics by socket type
        const buyResults = results.filter(r => r.socketType === 'buy');
        const sellResults = results.filter(r => r.socketType === 'sell');
        const defaultResults = results.filter(r => r.socketType === 'default');

        const stats = {
            total: results.length,
            passed: results.filter(r => r.passed).length,
            failed: results.filter(r => !r.passed).length,
            totalElapsed,
            avgElapsed: results.reduce((sum, r) => sum + r.elapsed, 0) / results.length,
            minElapsed: Math.min(...results.map(r => r.elapsed)),
            maxElapsed: Math.max(...results.map(r => r.elapsed)),
            bySocket: {
                buy: {
                    total: buyResults.length,
                    passed: buyResults.filter(r => r.passed).length,
                    avgElapsed: buyResults.reduce((sum, r) => sum + r.elapsed, 0) / buyResults.length || 0
                },
                sell: {
                    total: sellResults.length,
                    passed: sellResults.filter(r => r.passed).length,
                    avgElapsed: sellResults.reduce((sum, r) => sum + r.elapsed, 0) / sellResults.length || 0
                },
                default: {
                    total: defaultResults.length,
                    passed: defaultResults.filter(r => r.passed).length,
                    avgElapsed: defaultResults.reduce((sum, r) => sum + r.elapsed, 0) / defaultResults.length || 0
                }
            }
        };

        this.logger.info('Concurrent test suite completed', {
            ...stats,
            successRate: `${((stats.passed / stats.total) * 100).toFixed(2)}%`,
            throughput: `${(stats.total / (totalElapsed / 1000)).toFixed(2)} messages/sec`,
            buySuccessRate: buyResults.length > 0 ? `${((stats.bySocket.buy.passed / stats.bySocket.buy.total) * 100).toFixed(2)}%` : 'N/A',
            sellSuccessRate: sellResults.length > 0 ? `${((stats.bySocket.sell.passed / stats.bySocket.sell.total) * 100).toFixed(2)}%` : 'N/A'
        });

        this.logger.debug('Detailed results by socket type:', {
            buyResults: buyResults.map(r => ({
                name: r.name,
                success: r.success,
                elapsed: r.elapsed
            })),
            sellResults: sellResults.map(r => ({
                name: r.name,
                success: r.success,
                elapsed: r.elapsed
            })),
            defaultResults: defaultResults.map(r => ({
                name: r.name,
                success: r.success,
                elapsed: r.elapsed
            }))
        });
    }

    async testMassiveConcurrency(messageCount: number = 100, socketType?: 'buy' | 'sell'): Promise<void> {
        this.logger.info('Starting massive concurrency test', {
            messageCount,
            socketType: socketType || 'mixed',
            warning: 'This may stress the server'
        });

        // Generate test messages with alternating socket types
        const messages = Array.from({ length: messageCount }, (_, i) => {
            const types: Array<'buy' | 'sell' | undefined> = socketType ? [socketType] : ['buy', 'sell', undefined];
            const type = types[i % types.length];
            
            return {
                name: `Massive-${i + 1}-${type || 'default'}`,
                message: i % 3 === 0 
                    ? 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v'
                    : i % 3 === 1
                    ? 'So11111111111111111111111111111111111111112'
                    : 'This is a test message without addresses',
                socketType: type
            };
        });

        const startTime = Date.now();
        const results = await this.sendMessagesConcurrently(messages);
        const totalElapsed = Date.now() - startTime;

        const stats = {
            total: results.length,
            successful: results.filter(r => r.success).length,
            failed: results.filter(r => !r.success).length,
            totalElapsed,
            avgElapsed: results.reduce((sum, r) => sum + r.elapsed, 0) / results.length
        };

        this.logger.info('Massive concurrency test completed', {
            ...stats,
            successRate: `${((stats.successful / stats.total) * 100).toFixed(2)}%`,
            throughput: `${(stats.total / (totalElapsed / 1000)).toFixed(2)} messages/sec`
        });
    }

    async interactiveTest(): Promise<void> {
        this.logger.info('Starting interactive test mode');
        
        const readline = await import('readline/promises');
        const rl = readline.createInterface({
            input: process.stdin,
            output: process.stdout
        });

        while (true) {
            console.log('\n=== Interactive Test Mode ===');
            console.log('Current socket:', this.getCurrentSocketInfo());
            console.log('Commands:');
            console.log('  message - Send message to current socket');
            console.log('  buy <message> - Send to buy socket');
            console.log('  sell <message> - Send to sell socket');
            console.log('  switch <default|buy|sell> - Change current socket');
            console.log('  status - Get status from all sockets');
            console.log('  test-buysell <address> - Test buy then sell');
            console.log('  quit - Exit interactive mode');
            
            const input = await rl.question('\nEnter command: ');
            
            if (input.toLowerCase() === 'quit') {
                this.logger.info('Exiting interactive mode');
                break;
            }

            try {
                if (input.startsWith('switch ')) {
                    const socketType = input.substring(7).trim() as 'default' | 'buy' | 'sell';
                    if (['default', 'buy', 'sell'].includes(socketType)) {
                        this.setSocketType(socketType);
                        console.log(`Switched to ${socketType} socket`);
                    } else {
                        console.log('Invalid socket type. Use: default, buy, or sell');
                    }
                    continue;
                }

                if (input.startsWith('buy ')) {
                    const message = input.substring(4).trim();
                    this.logger.info('Sending to buy socket', { message });
                    
                    const response = await this.sendMessage(message, 'buy');
                    
                    console.log('\nBuy Socket Response:');
                    console.log(JSON.stringify(response, null, 2));
                    continue;
                }

                if (input.startsWith('sell ')) {
                    const message = input.substring(5).trim();
                    this.logger.info('Sending to sell socket', { message });
                    
                    const response = await this.sendMessage(message, 'sell');
                    
                    console.log('\nSell Socket Response:');
                    console.log(JSON.stringify(response, null, 2));
                    continue;
                }

                if (input.startsWith('test-buysell ')) {
                    const mintAddress = input.substring(13).trim();
                    console.log(`Testing buy then sell for: ${mintAddress}`);
                    
                    const results = await this.testBuyThenSellSequence(mintAddress);
                    
                    console.log('\nBuy-Sell Sequence Results:');
                    results.forEach(result => {
                        console.log(`${result.passed ? '✓' : '✗'} ${result.name}: ${result.success ? 'Success' : 'Failed'} (${result.elapsed}ms)`);
                    });
                    continue;
                }

                if (input.toLowerCase() === 'status') {
                    const status = await this.getServerStatus();
                    
                    console.log('\nServer Status:');
                    console.log(JSON.stringify(status, null, 2));
                    continue;
                }

                // Default: send to current socket
                this.logger.info('Processing user input', {
                    message: input,
                    socketType: this.currentSocketType,
                    socketPath: this.socketPath
                });
                
                const response = await this.sendMessage(input);
                
                console.log('\nResponse:');
                console.log(JSON.stringify(response, null, 2));
                
            } catch (error) {
                this.logger.error('Interactive test error', error);
                console.error(`Error: ${error instanceof Error ? error.message : 'Unknown error'}`);
            }
        }

        rl.close();
    }
}

// Main execution with enhanced command line interface
async function main() {
    const mainLogger = new Logger({
        name: 'SPLTokenIPCClient-App',
        type: 'pretty',
        minLevel: 'info',
        displayFunctionName: false,
        displayFilePath: 'hidden',
    });

    mainLogger.info('Starting SPL Token IPC Client with multi-socket support');
    
    // Parse command line arguments
    const args = process.argv.slice(2);
    const command = args[0] || 'help';
    
    // Initialize client with configurable socket paths
    const client = new SPLTokenIPCClient(
        process.env.DEFAULT_SOCKET || '/tmp/spl_token_ipc.sock',
        process.env.BUY_SOCKET || '/tmp/spl_buy.sock',
        process.env.SELL_SOCKET || '/tmp/spl_sell.sock'
    );

    mainLogger.info('Socket configuration:', {
        default: client.getCurrentSocketInfo(),
        buy: '/tmp/spl_buy.sock',
        sell: '/tmp/spl_sell.sock'
    });

    // Check if sockets exist
    const sockets = [
        { path: '/tmp/spl_token_ipc.sock', type: 'default' },
        { path: '/tmp/spl_buy.sock', type: 'buy' },
        { path: '/tmp/spl_sell.sock', type: 'sell' }
    ];

    for (const socket of sockets) {
        try {
            await fs.access(socket.path);
            mainLogger.info('Socket found', { type: socket.type, path: socket.path });
        } catch (error) {
            mainLogger.warn('Socket not found', { type: socket.type, path: socket.path });
        }
    }

    // Command routing
    switch (command.toLowerCase()) {
        case 'help':
            console.log('\nSPL Token IPC Client - Multi-Socket Edition');
            console.log('============================================');
            console.log('\nCommands:');
            console.log('  help                      - Show this help');
            console.log('  interactive               - Interactive test mode');
            console.log('  single <message>          - Send single message to default socket');
            console.log('  buy <message>             - Send message to buy socket');
            console.log('  sell <message>            - Send message to sell socket');
            console.log('  test-buysell <address>    - Test buy then sell sequence');
            console.log('  test-switching            - Test socket switching');
            console.log('  test-both <address>       - Test both buy and sell sockets');
            console.log('  status                    - Get status from all sockets');
            console.log('  switch <type>             - Change default socket (default|buy|sell)');
            console.log('  concurrent                - Run concurrent test suite');
            console.log('  massive [count] [type]    - Massive concurrency test');
            console.log('\nExamples:');
            console.log('  bun ipc-client.ts buy EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v');
            console.log('  bun ipc-client.ts sell EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v');
            console.log('  bun ipc-client.ts test-buysell EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v');
            console.log('  bun ipc-client.ts switch buy');
            console.log('  bun ipc-client.ts single "status"');
            break;

        case 'interactive':
            await client.interactiveTest();
            break;

        case 'single':
            if (args.length < 2) {
                mainLogger.error('Missing message for single mode');
                process.exit(1);
            }
            const message = args.slice(1).join(' ');
            mainLogger.info('Sending single message to default socket', { message });
            const response = await client.sendMessage(message);
            console.log(JSON.stringify(response, null, 2));
            break;

        case 'buy':
            if (args.length < 2) {
                mainLogger.error('Missing message for buy command');
                process.exit(1);
            }
            const buyMessage = args.slice(1).join(' ');
            mainLogger.info('Sending to buy socket', { message: buyMessage });
            const buyResponse = await client.sendMessage(buyMessage, 'buy');
            console.log(JSON.stringify(buyResponse, null, 2));
            break;

        case 'sell':
            if (args.length < 2) {
                mainLogger.error('Missing message for sell command');
                process.exit(1);
            }
            const sellMessage = args.slice(1).join(' ');
            mainLogger.info('Sending to sell socket', { message: sellMessage });
            const sellResponse = await client.sendMessage(sellMessage, 'sell');
            console.log(JSON.stringify(sellResponse, null, 2));
            break;

        case 'test-buysell':
            if (args.length < 2) {
                mainLogger.error('Missing mint address for test-buysell');
                process.exit(1);
            }
            const mintAddress = args[1];
            const config = args[2] ? JSON.parse(args[2]) : undefined;
            mainLogger.info('Testing buy-then-sell sequence', { mintAddress });
            const sequenceResults = await client.testBuyThenSellSequence(mintAddress, config, config);
            console.log(JSON.stringify(sequenceResults, null, 2));
            break;

        case 'test-switching':
            mainLogger.info('Testing socket switching');
            const switchingResults = await client.testSocketSwitching();
            console.log(JSON.stringify(switchingResults, null, 2));
            break;

        case 'test-both':
            if (args.length < 2) {
                mainLogger.error('Missing mint address for test-both');
                process.exit(1);
            }
            const bothAddress = args[1];
            const bothConfig = args[2] ? JSON.parse(args[2]) : undefined;
            mainLogger.info('Testing both buy and sell sockets', { mintAddress: bothAddress });
            const bothResults = await client.testBuySellSocket(bothAddress, bothConfig);
            console.log(JSON.stringify(bothResults, null, 2));
            break;

        case 'status':
            mainLogger.info('Getting server status');
            const status = await client.getServerStatus();
            console.log(JSON.stringify(status, null, 2));
            break;

        case 'switch':
            if (args.length < 2) {
                mainLogger.error('Missing socket type for switch command');
                process.exit(1);
            }
            const socketType = args[1] as 'default' | 'buy' | 'sell';
            if (['default', 'buy', 'sell'].includes(socketType)) {
                client.setSocketType(socketType);
                console.log(`Switched to ${socketType} socket`);
            } else {
                console.log('Invalid socket type. Use: default, buy, or sell');
            }
            break;

        case 'concurrent':
            await client.testMultipleMessagesConcurrently();
            break;

        case 'massive':
            const count = parseInt(args[1] || '100', 10);
            const type = args[2] as 'buy' | 'sell' | undefined;
            await client.testMassiveConcurrency(count, type);
            break;

        default:
            // Default: run concurrent test suite
            await client.testMultipleMessagesConcurrently();
            break;
    }

    mainLogger.info('Application completed');
}

// Run if this is the main module
if (import.meta.url === `file://${process.argv[1]}`) {
    main().catch(error => {
        const errorLogger = new Logger({
            name: 'SPLTokenIPCClient-Error',
            type: 'pretty',
            minLevel: 'error'
        });
        errorLogger.fatal('Unhandled application error', error);
        process.exit(1);
    });
}

export { SPLTokenIPCClient, TestResult };