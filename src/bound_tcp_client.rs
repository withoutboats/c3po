use std::net::SocketAddr;

use core::net::TcpStream;
use core::reactor::Handle;
use proto::{BindClient, TcpClient, Connect as Connection};
use service::{Service, Connect};

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

impl<K, P> Connect<Handle> for BoundTcpClient<K, P>
where
    K: 'static,
    P: BindClient<K, TcpStream>,
    P::BindClient: Service<Request = P::ServiceRequest, Response = P::ServiceResponse, Error = P::ServiceError>,
{
    type Request = P::ServiceRequest;
    type Response = P::ServiceResponse;
    type Error = P::ServiceError;
    type Instance = P::BindClient;
    type Future = Connection<K, P>;

    fn connect(&self, handle: &Handle) -> Self::Future {
        self.client.connect(&self.addr, handle)
    }
}

pub trait ConnectionWrapper<Kind, P: BindClient<Kind, TcpStream>> {
    fn wrap(client: P::BindClient) -> Self;
}
