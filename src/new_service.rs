use std::io;
use std::net::SocketAddr;

use futures::Future;
use core::net::TcpStream;
use core::reactor::Handle;
use proto::{BindClient, TcpClient, Connect};
use service::Service;

pub trait NewService<H> {
    type Request;
    type Response;
    type Error;
    type Instance: Service<Request = Self::Request, Response = Self::Response, Error = Self::Error>;

    type Future: Future<Item = Self::Instance, Error = io::Error> + 'static;

    fn new_service(&self, handle: &H) -> Self::Future;
}

pub struct BoundTcpClient<K, P: BindClient<K, TcpStream>> {
    client: TcpClient<K, P>,
    addr: SocketAddr,
}

impl<K, P: BindClient<K, TcpStream>> BoundTcpClient<K, P> {
    pub fn new(proto: P, addr: SocketAddr) -> Self {
        BoundTcpClient {
            client: TcpClient::new(proto),
            addr: addr,
        }
    }
}

impl<K: 'static, P: BindClient<K, TcpStream>> NewService<Handle> for BoundTcpClient<K, P> {
    type Request = P::ServiceRequest;
    type Response = P::ServiceResponse;
    type Error = P::ServiceError;
    type Instance = P::BindClient;
    type Future = Connect<K, P>;

    fn new_service(&self, handle: &Handle) -> Self::Future {
        self.client.connect(&self.addr, handle)
    }
}
