import("MrxUtil")

function Create(srcObj, tConfig)
  if tConfig == nil then
    return
  end
  if not Object.IsAlive(tConfig.uVehicle) then
    Debug.Printf("ApcDrop:Create() target vehicle is not alive " .. tostring(tConfig.uVehicle))
    return
  end
  local self = {}
  setmetatable(self, srcObj)
  srcObj.__index = srcObj
  self.uVehicle = tConfig.uVehicle
  self.uDriver = Vehicle.GetDriver(self.uVehicle)
  self.inDest = _GetGuidIfString(tConfig.inDest)
  self.inDestType = tConfig.inDestType
  self.inSpeed = MrxUtil.SetDefault(tConfig.inSpeed, 0.8)
  self.outDest = _GetGuidIfString(tConfig.outDest)
  self.outDestType = tConfig.outDestType
  self.outSpeed = MrxUtil.SetDefault(tConfig.outSpeed, 0.8)
  self.squadName = tConfig.squadName
  self.squadTarget = _GetGuidIfString(tConfig.squadTarget)
  self.squadOrder = tConfig.squadOrder
  self.fDropDoneCallback = tConfig.fDropDoneCallback
  self.MaintainRotorSpeed = tConfig.MaintainRotorSpeed
  if self.inDest then
    local args = {
      AIGuid = self.uDriver,
      Priority = "HiPri",
      Target = self.inDest,
      Callback = self._DropCallback,
      CallbackData = {self},
      Force = true
    }
    if self.inDestType == "path" then
      args.Goal = "PathMove"
    elseif self.inDestType == "object" then
      args.Goal = "MoveTo"
    elseif self.inDestType == "coord" then
      args.Goal = "MoveToPos"
      args.Target = nil
      args.Location = self.inDest
    else
      Debug.Printf("MrxApcDrop:Create unknown inDestType: " .. tostring(self.inDestType))
    end
    self.bIsHeli = Object.HasLabel(self.uVehicle, "helicopter")
    if self.bIsHeli then
      args.Callback = nil
    end
    local h = Ai.Goal(args)
    if h then
      Ai.SetHaste(self.uDriver, self.inSpeed)
      if self.bIsHeli then
        Ai.Goal({
          AIGuid = self.uDriver,
          Goal = "HeliLand",
          Priority = "hiPri",
          Callback = self._DropCallback,
          CallbackData = {self}
        })
      end
    else
      Debug.Printf("MrxApcDrop:Create unable to set Ai.Goal.  Make sure vehicle/driver are alive and unhibernated!")
      return nil
    end
    self.eDeath = Event.Create(Event.ObjectDeath, {
      self.uDriver
    }, Cancel, {self})
  else
    Debug.Printf("MrxApcDrop: drop the gang immediately")
    self:DropCallback()
  end
  return self
end

function Cancel(self)
  Event.Delete(self.eDeath)
  Event.Delete(self.eExitDelay)
end

function _DropCallback(self)
  self.tRiders = Vehicle.GetRiders(self.uVehicle, "p")
  Ai.Deploy({
    Vehicle = self.uVehicle,
    Role = "Passenger",
    Priority = "HiPri",
    Force = true,
    MaintainRotorSpeed = self.MaintainRotorSpeed,
    Callback = self._DropCallback2,
    CallbackData = {self}
  })
end

function _DropCallback2(self)
  self.eExitDelay = nil
  if self.bIsHeli then
    Ai.Goal({
      AIGuid = self.uDriver,
      Goal = "HeliTakeoff",
      Priority = "hiPri"
    })
  end
  if self.outDest then
    local args = {
      AIGuid = self.uDriver,
      Priority = "HiPri",
      Target = self.outDest,
      Force = true
    }
    if self.outDestType == "path" then
      args.Goal = "PathMove"
      args.Start = "first"
    elseif self.outDestType == "object" then
      args.Goal = "MoveTo"
    elseif self.outDestType == "coord" then
      args.Goal = "MoveToPos"
      args.Target = nil
      args.Location = self.outDest
    else
      Debug.Printf("MrxApcDrop:Create unknown outDestType: " .. tostring(self.outDestType))
    end
    local h = Ai.Goal(args)
    if h then
      Ai.SetHaste(self.uDriver, self.outSpeed)
    else
      Debug.Printf("MrxApcDrop:_DropCallback unable to set Ai.Goal.  Make sure vehicle/driver are alive and unhibernated!")
    end
  end
  _CommandSquad(self)
end

function _CommandSquad(self)
  if self.squadName then
    Ai.Squad({
      Squad = self.squadName,
      Action = "AddUnits",
      Guids = self.tRiders
    })
    local h = Ai.Squad({
      Squad = self.squadName,
      Action = "AddCommand",
      Goal = "MoveWithinBoundary",
      Target = {
        Object.GetPosition(self.squadTarget)
      },
      Radius = 8,
      Style = self.squadOrder,
      Priority = "hiPri"
    })
    if h == 0 then
      Debug.Printf("MrxApcDrop:_DropCallback unable to issue MoveWithinBoundary command to Squad " .. tostring(self.squadName))
    end
  end
  if self.fDropDoneCallback then
    self.fDropDoneCallback(self.uVehicle, self.tRiders)
  end
  self.tRiders = nil
end

function _GetGuidIfString(param)
  if type(param) == "string" then
    return Pg.GetGuidByName(param)
  else
    return param
  end
end
