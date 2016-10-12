module SvgSlider exposing (..) 

-- import Platform exposing (Cmd, none) 
import Html exposing (Html)
import Json.Decode as JD exposing ((:=))
import Json.Encode as JE 
-- import Task
import Svg exposing (Svg, svg, rect, g, text, text', Attribute)
import Svg.Attributes exposing (..)
import Svg.Events exposing (onClick, onMouseUp, onMouseMove, onMouseDown, onMouseOut)
-- import NoDragEvents exposing (onClick, onMouseUp, onMouseMove, onMouseDown, onMouseOut)
import SvgThings exposing (Orientation(..)) 
import VirtualDom as VD
import SvgTouch as ST
import WebSocket
import String
import List
import Dict

type alias Spec = 
  { name: String
  , label: Maybe String
  , orientation: SvgThings.Orientation
  }

jsSpec : JD.Decoder Spec
jsSpec = JD.object3 Spec 
  ("name" := JD.string)
  (JD.maybe ("label" := JD.string)) 
  (("orientation" := JD.string) `JD.andThen` SvgThings.jsOrientation)

-- MODEL

type alias Model =
  { name : String
  , label: String
  , cid: SvgThings.ControlId 
  , rect: SvgThings.Rect
  , srect: SvgThings.SRect
  , orientation: SvgThings.Orientation
  , pressed: Bool
  , location: Float
  , sendaddr: String
  , textSvg: List (Svg ())
  , touchonly: Bool
  }

init: String -> SvgThings.Rect -> SvgThings.ControlId -> Spec
  -> (Model, Cmd msg)
init sendaddr rect cid spec =
  let ts = case spec.label of 
        Just lbtext -> SvgThings.calcTextSvg SvgThings.ff lbtext rect 
        Nothing -> []
    in
   (Model (spec.name)
          (Maybe.withDefault "" (spec.label))
          cid 
          rect
          (SvgThings.SRect (toString rect.x)
                           (toString rect.y)
                           (toString rect.w)
                           (toString rect.h))
          spec.orientation
          False 
          0.5 
          sendaddr
          ts
          False
  , Cmd.none
  )

buttColor: Bool -> String
buttColor pressed = 
  case pressed of 
    True -> "#f000f0"
    False -> "#60B5CC"

-- UPDATE

type Msg 
    = SvgPress JE.Value
    | SvgUnpress JE.Value 
    | NoOp 
    | Reply String 
    | SvgMoved JE.Value
    | SvgTouch ST.Msg 
    | SvgUpdate UpdateMessage

getX : JD.Decoder Int
getX = "clientX" := JD.int 

getY : JD.Decoder Int
getY = "clientY" := JD.int 

type UpdateType 
  = Press
  | Unpress

type alias UpdateMessage = 
  { controlId: SvgThings.ControlId
  , updateType: Maybe UpdateType
  , location: Maybe Float
  , label: Maybe String
  }

encodeUpdateMessage: UpdateMessage -> JD.Value
encodeUpdateMessage um = 
  let outlist1 = [ ("controlType", JE.string "slider") 
            , ("controlId", SvgThings.encodeControlId um.controlId) ]
      outlist2 = case um.updateType of 
                  Just ut -> List.append outlist1 [ ("state", encodeUpdateType ut) ]
                  Nothing -> outlist1
      outlist3 = case um.location of 
                  Just loc -> List.append outlist2 [ ("location", JE.float loc)]
                  Nothing -> outlist2
      outlist4 = case um.label of 
                  Just txt -> List.append outlist3 [ ("label", JE.string txt)]
                  Nothing -> outlist3
    in JE.object outlist4

{-  JE.object [ ("controlType", JE.string "slider") 
            , ("controlId", SvgThings.encodeControlId um.controlId) 
            , ("updateType", encodeUpdateType um.updateType) 
            , ("location", (JE.float um.location))
            ]
  -}

encodeUpdateType: UpdateType -> JD.Value
encodeUpdateType ut = 
  case ut of 
    Press -> JE.string "Press"
    Unpress -> JE.string "Unpress"

jsUpdateMessage : JD.Decoder UpdateMessage
jsUpdateMessage = JD.object4 UpdateMessage 
  ("controlId" := SvgThings.decodeControlId) 
  (JD.maybe (("state" := JD.string) `JD.andThen` jsUpdateType))
  (JD.maybe ("location" := JD.float))
  (JD.maybe ("label" := JD.string)) 
  
jsUpdateType : String -> JD.Decoder UpdateType 
jsUpdateType ut = 
  case ut of 
    "Press" -> JD.succeed Press
    "Unpress" -> JD.succeed Unpress 
    _ -> JD.succeed Unpress 

-- get mouse/whatever location from the json message, 
-- compute slider location from that.
getLocation: Model -> JD.Value -> Result String Float
getLocation model v = 
  case model.orientation of 
    SvgThings.Horizontal ->
      case (JD.decodeValue getX v) of 
        Ok i -> Ok ((toFloat (i - model.rect.x)) 
                    / toFloat model.rect.w)
        Err e -> Err e
    SvgThings.Vertical -> 
      case (JD.decodeValue getY v) of 
        Ok i -> Ok ((toFloat (i - model.rect.y)) 
                    / toFloat model.rect.h)
        Err e -> Err e

update : Msg -> Model -> (Model, Cmd Msg)
update msg model =
  case msg of
    SvgPress v -> 
      case (getLocation model v) of 
        Ok l -> updsend model (Just Press) l
        _ -> (model, Cmd.none)
    SvgUnpress v -> 
      case model.pressed of 
        True -> updsend model (Just Unpress) model.location 
        False -> (model, Cmd.none)
    NoOp -> (model, Cmd.none)
    Reply s -> ({model | name = s}, Cmd.none)
    SvgMoved v ->
      case model.pressed of 
        True -> 
          case (getLocation model v) of 
            Ok l -> 
               -- Debug.log "not blah" (updsend model Nothing l)
               updsend model Nothing l
            _ -> (model, Cmd.none)
        False -> (model, Cmd.none)
    SvgUpdate um -> 
      -- sanity check for ids?  or don't.
      let mod = { model | 
            pressed = (case um.updateType of 
                Just Press -> True              
                Just Unpress -> False
                _ -> model.pressed),
            location = (case um.location of 
              Just loc -> loc
              Nothing -> model.location),
            label = (case um.label of 
              Just txt -> txt
              Nothing -> model.label) }
        in
      (mod, Cmd.none)
    SvgTouch stm -> 
      case ST.extractFirstRectTouchSE stm model.rect of
        Nothing -> 
          if model.pressed then
            updsend model (Just Unpress) model.location
          else 
            (model, Cmd.none )
        Just touch -> 
          case model.orientation of 
            SvgThings.Horizontal -> 
              let loc = (touch.x - (toFloat model.rect.x)) 
                         / toFloat model.rect.w in 
              if model.pressed then
                updsend model (Just Press) loc
              else 
                updsend model Nothing loc
            SvgThings.Vertical -> 
              let loc = (touch.y - (toFloat model.rect.y)) 
                         / toFloat model.rect.h in 
              if model.pressed then
                updsend model (Just Press) loc
              else 
                updsend model Nothing loc


updsend: Model -> Maybe UpdateType -> Float -> (Model, Cmd Msg)
updsend model mbut loc = 
  let bLoc = if (loc > 1.0) then 
                1.0
             else if (loc < 0.0) then
                0.0
             else
                loc 
      prest = mbut /= Just Unpress
  in
  -- if nothing changed, no message.
  if (model.location == bLoc && model.pressed == prest) then 
    (model, Cmd.none)
  else
    let um = JE.encode 0 
              (encodeUpdateMessage 
                (UpdateMessage model.cid mbut (Just bLoc) Nothing)) in
    ( {model | location = bLoc, pressed = prest }
      ,(WebSocket.send model.sendaddr um) )
         

resize: Model -> SvgThings.Rect -> (Model, Cmd Msg)
resize model rect = 
  let ts = SvgThings.calcTextSvg SvgThings.ff model.label rect in
  ({ model | rect = rect
           , srect = (SvgThings.SRect (toString rect.x)
                                     (toString rect.y)
                                     (toString rect.w)
                                     (toString rect.h))
           , textSvg = ts
            }
  , Cmd.none)
 
-- VIEW

-- try VD.onWithOptions for preventing scrolling on touchscreens and 
-- etc. See virtualdom docs.

sliderEvt: String -> (JD.Value -> Msg) -> VD.Property Msg
sliderEvt evtname mkmsg =
    VD.onWithOptions evtname (VD.Options True True) (JD.map (\v -> mkmsg v) JD.value)

onMouseDown = sliderEvt "mousedown" SvgPress
onMouseMove = sliderEvt "mousemove" SvgMoved
onMouseLeave = sliderEvt "mouseleave" SvgUnpress
onMouseUp = sliderEvt "mouseup" SvgUnpress

onTouchStart = sliderEvt "touchstart" (\e -> SvgTouch (ST.SvgTouchStart e))
onTouchEnd = sliderEvt "touchend" (\e -> SvgTouch (ST.SvgTouchEnd e))
onTouchCancel = sliderEvt "touchcancel" (\e -> SvgTouch (ST.SvgTouchCancel e))
onTouchLeave = sliderEvt "touchleave" (\e -> SvgTouch (ST.SvgTouchLeave e))
onTouchMove = sliderEvt "touchmove" (\e -> SvgTouch (ST.SvgTouchMove e))

buildEvtHandlerList: Bool -> List (VD.Property Msg)
buildEvtHandlerList touchonly = 
 let te =  [ onTouchStart
            , onTouchEnd 
            , onTouchCancel 
            , onTouchLeave 
            , onTouchMove ] 
     me = [ onMouseDown 
          , onMouseUp 
          , onMouseLeave
          , onMouseMove ] in
  if touchonly then te else (List.append me te)

view : Model -> Svg Msg
view model =
  let (sx, sy, sw, sh) = case model.orientation of 
                             SvgThings.Vertical -> 
                                (model.srect.x
                                ,toString ((round (model.location * toFloat (model.rect.h))) + model.rect.y)
                                ,model.srect.w
                                ,"3")
                             SvgThings.Horizontal -> 
                                (toString ((round (model.location * toFloat (model.rect.w))) + model.rect.x)
                                ,model.srect.y
                                ,"3"
                                ,model.srect.h)
      evtlist = buildEvtHandlerList model.touchonly
   in
  g evtlist 
    [ rect
        [ x model.srect.x
        , y model.srect.y 
        , width model.srect.w
        , height model.srect.h
        , rx "2"
        , ry "2"
        , style "fill: #F1F1F1;"
        ]
        []
    , rect
        [ x sx 
        , y sy 
        , width sw
        , height sh 
        , rx "2"
        , ry "2"
        , style ("fill: " ++ buttColor(model.pressed) ++ ";")
        ]
        []
    , VD.map (\_ -> NoOp) (g [ ] model.textSvg)
    ]


