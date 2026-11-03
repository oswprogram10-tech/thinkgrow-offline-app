import Foundation
import Security
import CryptoKit
import Tauri

struct VaultArguments: Decodable {let data: String; let entropy: String}
enum VaultError: Error {case unavailable}

class VaultPlugin: Plugin {
    private let queue = DispatchQueue(label: "com.thinkgrow.offline.vault")
    private func key(create: Bool) throws -> SymmetricKey {
        guard let bundle = Bundle.main.bundleIdentifier else {throw VaultError.unavailable}
        let query: [String:Any] = [kSecClass as String:kSecClassGenericPassword,
            kSecAttrService as String:bundle + ".thinkgrow-vault.v1",kSecAttrAccount as String:"protected-file-key",
            kSecAttrSynchronizable as String:false]
        var lookup = query;lookup[kSecReturnData as String]=true;lookup[kSecMatchLimit as String]=kSecMatchLimitOne
        lookup[kSecUseAuthenticationUI as String]=kSecUseAuthenticationUIFail
        var result: CFTypeRef?;let status = SecItemCopyMatching(lookup as CFDictionary,&result)
        if status == errSecSuccess {
            guard var bytes = result as? Data,bytes.count == 32 else {throw VaultError.unavailable}
            defer {bytes.resetBytes(in: 0..<bytes.count)}
            return SymmetricKey(data:bytes)
        }
        guard status == errSecItemNotFound && create else {throw VaultError.unavailable}
        var bytes=Data(count:32);let random = bytes.withUnsafeMutableBytes {SecRandomCopyBytes(kSecRandomDefault,32,$0.baseAddress!)}
        guard random == errSecSuccess else {throw VaultError.unavailable}
        defer {bytes.resetBytes(in:0..<bytes.count)}
        var item=query;item[kSecValueData as String]=bytes;item[kSecAttrAccessible as String]=kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        let inserted=SecItemAdd(item as CFDictionary,nil)
        if inserted == errSecDuplicateItem {return try key(create:false)}
        guard inserted == errSecSuccess else {throw VaultError.unavailable}
        return SymmetricKey(data:bytes)
    }
    private func operation(_ invoke:Invoke,encrypt:Bool) throws {
        let args=try invoke.parseArgs(VaultArguments.self)
        queue.async {
            do {
                guard args.data.count <= 2*1024*1024,let data=Data(base64Encoded:args.data),
                    let entropy=Data(base64Encoded:args.entropy),String(data:entropy,encoding:.utf8)==Bundle.main.bundleIdentifier else {throw VaultError.unavailable}
                let key=try self.key(create:encrypt)
                let output:Data
                if encrypt {
                    guard !data.isEmpty && data.count <= 1048576+8192 else {throw VaultError.unavailable}
                    let sealed=try AES.GCM.seal(data,using:key,authenticating:entropy)
                    guard let combined=sealed.combined else {throw VaultError.unavailable}
                    output=Data([1])+combined
                } else {
                    guard data.count >= 29 && data.count <= 1048576+8192 && data.first==1 else {throw VaultError.unavailable}
                    output=try AES.GCM.open(AES.GCM.SealedBox(combined:Data(data.dropFirst())),using:key,authenticating:entropy)
                }
                invoke.resolve(["data":output.base64EncodedString()])
            }catch {invoke.reject("Native protected operation failed; existing data retained")}
        }
    }
    @objc func seal(_ invoke:Invoke) throws {try operation(invoke,encrypt:true)}
    @objc func unseal(_ invoke:Invoke) throws {try operation(invoke,encrypt:false)}
    @objc func redeem(_ invoke:Invoke) throws {try post(invoke,path:"devices/redeem",limit:262144,signed:false)}
    @objc func lessons(_ invoke:Invoke) throws {try post(invoke,path:"lessons/prepare",limit:1048576,signed:true)}
    private func post(_ invoke:Invoke,path:String,limit:Int,signed:Bool) throws {
        let args=try invoke.parseArgs(VaultArguments.self)
        guard args.entropy.isEmpty,let data=Data(base64Encoded:args.data),data.count <= (signed ? 131072 : 16384),
            let request=try JSONSerialization.jsonObject(with:data) as? [String:Any] else {throw VaultError.unavailable}
        var bound=request
        if signed {
            guard let payload=request["payload"] as? String,let raw=payload.data(using:.utf8),let parsed=try JSONSerialization.jsonObject(with:raw) as? [String:Any] else {throw VaultError.unavailable}
            bound=parsed
        }
        guard bound["app_id"] as? String==Bundle.main.bundleIdentifier else {throw VaultError.unavailable}
        EnrollmentExchange(invoke:invoke,body:data,path:path,limit:limit).start()
    }
}
private class EnrollmentExchange:NSObject,URLSessionDataDelegate {
    let invoke:Invoke;let body:Data;let path:String;let limit:Int
    var session:URLSession?;var response=Data();var finished=false
    init(invoke:Invoke,body:Data,path:String,limit:Int){self.invoke=invoke;self.body=body;self.path=path;self.limit=limit}
    func start(){
        let configuration=URLSessionConfiguration.ephemeral
        configuration.httpCookieStorage=nil;configuration.httpShouldSetCookies=false
        configuration.timeoutIntervalForRequest=20;configuration.timeoutIntervalForResource=25
        let serial=OperationQueue();serial.maxConcurrentOperationCount=1
        session=URLSession(configuration:configuration,delegate:self,delegateQueue:serial)
        var request=URLRequest(url:URL(string:"https://schoolclassscorecheck.web.app/api/offline/v1/"+path)!)
        request.httpMethod="POST";request.httpBody=body;request.setValue("application/json",forHTTPHeaderField:"Content-Type")
        session?.dataTask(with:request).resume()
    }
    func finish(_ success:Bool){
        guard !finished else {return};finished=true
        if success {invoke.resolve(["data":response.base64EncodedString()])}else{invoke.reject("Registration request failed; existing data retained")}
        session?.invalidateAndCancel();session=nil
    }
    func urlSession(_ session:URLSession,task:URLSessionTask,willPerformHTTPRedirection response:HTTPURLResponse,newRequest request:URLRequest,completionHandler:@escaping(URLRequest?)->Void){completionHandler(nil)}
    func urlSession(_ session:URLSession,dataTask:URLSessionDataTask,didReceive response:URLResponse,completionHandler:@escaping(URLSession.ResponseDisposition)->Void){
        guard let http=response as? HTTPURLResponse,[200,201].contains(http.statusCode),response.expectedContentLength<=limit else {completionHandler(.cancel);finish(false);return}
        completionHandler(.allow)
    }
    func urlSession(_ session:URLSession,dataTask:URLSessionDataTask,didReceive data:Data){
        guard response.count+data.count<=limit else {finish(false);return};response.append(data)
    }
    func urlSession(_ session:URLSession,task:URLSessionTask,didCompleteWithError error:Error?){finish(error==nil)}
}
@_cdecl("init_plugin_thinkgrow_vault")
public func initPlugin()->Plugin {VaultPlugin()}
