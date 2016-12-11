
extern crate websocket;

use std::thread;
use std::sync::{Arc, Mutex};
use std::fs::File;
use std::path::Path;
use std::io::Read;
use std::io::Write;
use std::io::{Error,ErrorKind};
use std::string::*;

use websocket::{Server, Message, Sender, Receiver};
use websocket::header::WebSocketProtocol;
use websocket::message::Type;

extern crate iron;

use iron::prelude::*;
use iron::status;
use iron::mime::Mime;

#[macro_use]
mod tryopt;
mod stringerror;
mod controls;
mod broadcaster;
mod string_defaults; 
pub mod control_updates;

extern crate serde_json;
use serde_json::Value;


fn load_string(file_name: &str) -> Result<String, Box<std::error::Error> >
{
  let path = &Path::new(&file_name);
  let mut inf = try!(File::open(path));
  let mut result = String::new();
  try!(inf.read_to_string(&mut result));
  Ok(result)
}

fn write_string(text: &str, file_name: &str) -> Result<(), Box<std::error::Error> >
{
  let path = &Path::new(&file_name);
  let mut inf = try!(File::create(path));
  match inf.write(text.as_bytes()) { 
    Ok(_) => Ok(()),
    Err(e) => Err(Box::new(e)),
    }
}

pub struct ControlInfo {
  cm: controls::ControlMap,
  cnm: controls::ControlNameMap,
  guijson: String,
}

pub struct ControlServer<'a> { 
  ci: Arc<Mutex<ControlInfo>>,
  bc: broadcaster::Broadcaster,
  on_update_received: &'a ControlUpdateProcessor,
}

pub trait ControlUpdateProcessor { 
  fn on_update_received(&self, &control_updates::UpdateMsg) -> ();
}

impl<'a> ControlServer<'a> { 
  fn get_cid_by_name(&self, name: &str) -> Option<Vec<i32> > {
    let guard = match self.ci.lock() {
      Ok(guard) => guard,
      Err(poisoned) => poisoned.into_inner(),
    };

    match guard.cnm.get(name) {
      Some(cid) => Some(cid.clone()),
      _ => None,
    }
  }

  pub fn get_osc_name(&self, id: &Vec<i32>) -> Option<String>
  {
    let ci = match self.ci.lock() {
      Ok(guard) => guard,
      Err(poisoned) => poisoned.into_inner(),
    };

    match ci.cm.get(id) {
      Some(ctrl) => Some(String::from(ctrl.oscname())),
      _ => None,
    }

    
  }

  pub fn make_update_msg(&self, name: &str) -> Option<control_updates::UpdateMsg> {
    let guard = match self.ci.lock() {
      Ok(guard) => guard,
      Err(poisoned) => poisoned.into_inner(),
    };

    match guard.cnm.get(name) {
      Some(cid) =>
        match guard.cm.get(cid) {
          Some(ctrl) => ctrl.empty_update(),
          None => None,
        }, 
      _ => None,
    }
  }  
  pub fn update(&self, updmsg: &control_updates::UpdateMsg) {
    let mut ci = match self.ci.lock() {
      Ok(guard) => guard,
      Err(poisoned) => poisoned.into_inner(),
    };

    match ci.cm.get_mut(controls::get_um_id(&updmsg)) {
    Some(ctl) => {
      (*ctl).update(&updmsg);
      let val = controls::encode_update_message(&updmsg); 
      match serde_json::ser::to_string(&val) { 
        Ok(s) => self.bc.broadcast(Message::text(s)), 
        Err(_) => ()
        }
      }
    None => (),
    }
  }
  pub fn load_gui_string(&self, guistring: &str) -> Result<(), Box<std::error::Error> >
  {
    match serde_json::from_str(guistring) { 
      Ok(guival) => { 
        match controls::deserialize_root(&guival) {
          Ok(controltree) => { 
            println!("new control layout recieved!");

            println!("title: {} count: {} ", 
              controltree.title, controltree.root_control.control_type());
            println!("controls: {:?}", controltree.root_control);

            // from control tree, make a map of ids->controls.
            // let mapp = controls::make_control_map(&*controltree.root_control);
            // let cnm = controls::control_map_to_name_map(&mapp);
  /*
            sci.cm = mapp;
            sci.guijson = guistring.to_string();
            bc.broadcast(Message::text(guistring.to_string()));


  pub struct ControlInfo {
    cm: controls::ControlMap,
    cnm: controls::ControlNameMap,
    guijson: String,
  }


  */
            let mut guard = match self.ci.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };

            (*guard).cm = controls::make_control_map(&*controltree.root_control);
            (*guard).cnm = controls::control_map_to_name_map(&(*guard).cm);
            (*guard).guijson = guistring.to_string();
             
            // send the updated gui string to all clients.
            self.bc.broadcast(Message::text(guistring.to_string()));
            
            Ok(())
          },
          Err(e) => { 
            println!("error reading guiconfig from json: {:?}", e);
            let s = format!("error reading guiconfig json: {:?}", e);
            Err(Box::new(Error::new(ErrorKind::Other, s))) },
        }
      },
      Err(e) => {
        // println!("error reading guiconfig json: {:?}", e);
        let s = format!("error reading guiconfig json: {:?}", e);
        Err(Box::new(Error::new(ErrorKind::Other, s))) },
    }
  }

}
   

//  iron-server-canceler: ???
//  websocket-thread-canceler: ???
//  fn lookup_control(name) -> index
//  fn modify_control(index, ftn) -> result   <-- will this work with types??  have to cast.
//  load_guistring


//  how about ftns like the ones for faust, for building the gui tree?
//  addbutton
//  addslider
//  addlabel
//  startsizer
//  endsizer



pub fn startserver<'a>(guistring: &str, 
    on_update_received: &'a ControlUpdateProcessor,
    ip: &str, 
    http_port: &str, 
    websockets_port: &str, 
    htmltemplatefile: Option<&str> ) -> Result<ControlServer<'a>, Box<std::error::Error> >
{
    let mut http_ip = String::from(ip);
    http_ip.push_str(":");
    http_ip.push_str(&http_port);

    let mut websockets_ip = String::from(ip);
    websockets_ip.push_str(":");
    websockets_ip.push_str(&websockets_port);
    
    let htmltemplate = {  
      match htmltemplatefile {
        Some(fname) => { 
          try!(load_string(fname))
        }
        None => string_defaults::MAIN_HTML.to_string()
      }
    };
    
    let htmlstring = htmltemplate.replace("<websockets-port>", &websockets_port);

    // println!("{}", htmlstring);

    // let guistring = try!(load_string(&guifilename[..]));
    // let guival: Value = try!(serde_json::from_str(&guistring[..])); 
    let guival: Value = try!(serde_json::from_str(guistring)); 

    let blah = try!(controls::deserialize_root(&guival));

    println!("title: {} rootcontroltype: {} ", 
      blah.title, blah.root_control.control_type());
    println!("controls: {:?}", blah.root_control);

    // from control tree, make a map of ids->controls.
    let mapp = controls::make_control_map(&*blah.root_control);
    let cnm = controls::control_map_to_name_map(&mapp);
    // let guijson = guistring.clone();

    let ci = ControlInfo { cm: mapp, cnm: cnm, guijson: String::new() + guistring };

    let cmshare = Arc::new(Mutex::new(ci));
    let wscmshare = cmshare.clone();
    // let oscsocket = try!(UdpSocket::bind(&oscrecvip[..]));
    // for sending, bind to this.  if we bind to localhost, we can't
    // send messages to other machines.  
    // let oscsendsocket = try!(UdpSocket::bind("0.0.0.0:0"));
    let bc = broadcaster::Broadcaster::new();
    // let wsos = try!(oscsendsocket.try_clone());
    let wsbc = bc.clone();
    // let wsoscsendip = oscsendip.clone();

    let cs_ret = ControlServer { ci: cmshare, 
                                 bc: bc,
                                 on_update_received: on_update_received,
                               };

    /*
    thread::spawn(move || { 
      match oscmain(oscsocket, bc, cmshare) {
        Err(e) => println!("oscmain exited with error: {:?}", e),
        Ok(_) => (),
      }
      }); 
    */

    // Spawn a thread for the websockets handler.
    thread::spawn(move || { 
      match websockets_main(websockets_ip, wscmshare, wsbc, on_update_received) {
        Ok(_) => (),
        Err(e) => println!("error in websockets_main: {:?}", e),
      }
    });

    thread::spawn(move || { 
      // use this thread for the web server.
      Iron::new(move | _: &mut Request| {
          let content_type = "text/html".parse::<Mime>().unwrap();
          Ok(Response::with((content_type, status::Ok, &*htmlstring)))
      }).http(&http_ip[..]);
      // return when the web server dies, if it ever does. 
      });
      
    Ok(cs_ret)

}

// need to lock the control structs and stuff, refresh them, then send out the 
// updates.

// TODO: refactor to return a (rx/sx) pair for sending, recieving messages.
// library users start the websockets_main and get that pair of things.
// then, can send the various control structs and receive the messages.  
fn websockets_main<'a>( ipaddr: String, 
                    ci: Arc<Mutex<ControlInfo>>,
                    broadcaster: broadcaster::Broadcaster,
                    on_update_received: &'a ControlUpdateProcessor,
                    )
                  -> Result<(), Box<std::error::Error> >
{
	let server = try!(Server::bind(&ipaddr[..]));

	for connection in server {

    println!("new websockets connection!");
		// Spawn a new thread for each connection.
    
    let sci = ci.clone();
//    let osock = try!(oscsocket.try_clone());
 //   let osend = oscsendip.clone();
    let broadcaster = broadcaster.clone();

    let conn = try!(connection);
    thread::spawn(move || {
      match websockets_client(conn,
                            sci,
                            broadcaster,
                            on_update_received) {
        Ok(_) => (), 
        Err(e) => {
          println!("error in websockets thread: {:?}", e);
          ()
        },
      }
    });
  } 

  Ok(())
}

fn websockets_client(connection: websocket::server::Connection<websocket::stream::WebSocketStream, websocket::stream::WebSocketStream>,
                    ci: Arc<Mutex<ControlInfo>>,
                    mut broadcaster: broadcaster::Broadcaster, 
                    on_update_received: fn(&control_updates::UpdateMsg) -> (),
                    ) -> Result<(), Box<std::error::Error> >
{
  // Get the request
  let request = try!(connection.read_request());
  // Keep the headers so we can check them
  let headers = request.headers.clone(); 
  
  try!(request.validate()); // Validate the request
  
  let mut response = request.accept(); // Form a response
  
  if let Some(&WebSocketProtocol(ref protocols)) = headers.get() {
    if protocols.contains(&("rust-websocket".to_string())) {
      // We have a protocol we want to use
      response.headers.set(WebSocketProtocol(vec!["rust-websocket".to_string()]));
    }
  }
  
  let mut client = try!(response.send()); // Send the response
  
  let ip = try!(client.get_mut_sender()
                  .get_mut()
                  .peer_addr());
  
  println!("Websocket connection from {}", ip);

  // send up the json of the current controls.
  {
    let sci = ci.lock().unwrap();

    let updarray = controls::cm_to_update_array(&sci.cm);
  
    // build json message containing both guijson and the updarray.
    // let updvals = updarray.into_iter().map(|x|{controls::encode_update_message(&x)}).collect();

    let mut updvals = Vec::new();

    for upd in updarray { 
      let um = controls::encode_update_message(&upd);
      updvals.push(um);
    }
   
    let mut guival: Value = try!(serde_json::from_str(&sci.guijson[..]));

    match guival.as_object_mut() {
      Some(obj) => {
        obj.insert("state".to_string(), Value::Array(updvals));
        ()
      },
      None => (),
    }
  
    let guistring = try!(serde_json::ser::to_string(&guival));
    let message = Message::text(guistring);
    try!(client.send_message(&message));
  }
 
  let (sender, mut receiver) = client.split();

  let sendmeh = Arc::new(Mutex::new(sender));
  
  broadcaster.register(sendmeh.clone());      
  
  for msg in receiver.incoming_messages() {
    let message: Message = try!(msg);
    // println!("message: {:?}", message);

    match message.opcode {
      Type::Close => {
        let message = Message::close();
        // let mut sender = try!(sendmeh.lock());
        let mut sender = sendmeh.lock().unwrap();
        try!(sender.send_message(&message));
        println!("Client {} disconnected", ip);
        return Ok(());
      }
      Type::Ping => {
        println!("Message::Ping(data)");
        let message = Message::pong(message.payload);
        let mut sender = sendmeh.lock().unwrap();
        try!(sender.send_message(&message));
      }
      Type::Text => {
        let u8 = message.payload.to_owned();
        let str = try!(std::str::from_utf8(&*u8));
        let jsonval: Value = try!(serde_json::from_str(str));
        let s_um = controls::decode_update_message(&jsonval);
        match s_um { 
          Some(updmsg) => {
            let mut sci  = ci.lock().unwrap();
            let mbcntrl = sci.cm.get_mut(controls::get_um_id(&updmsg));
            match mbcntrl {
              Some(cntrl) => {
                (*cntrl).update(&updmsg);
                broadcaster.broadcast_others(&ip, Message::text(str));
                
                // TODO: callback ftn??  
                /*
                match ctrl_update_to_osc(&updmsg, &**cntrl) { 
                  Ok(v) => match oscsocket.send_to(&v, &oscsendip[..]) {
                    Ok(_) => (),
                    Err(e) => 
                      println!("error sending osc message: {:?}", e),
                    },
                  Err(e) => 
                    println!("error building osc message: {:?}", e),
                };
                */

                on_update_received(&updmsg);

                println!("websockets control update recieved: {:?}", updmsg);
                ()
              },
              None => println!("none"),
            }
          },
          _ => println!("decode_update_message failed on websockets msg: {:?}", message),
        }
      },
      _ => { 
        println!("unknown websockets msg: {:?}", message);
      }
    }
  }

  Ok(())
}
