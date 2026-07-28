inherit("MrxSupportDesignator")

function Init()
  Pg.LoadAsset("global_weapon_sw500", "model")
end

function Deinit()
  Pg.UnloadAsset("global_weapon_sw500", "model")
end

function Create(self, oNewDesignator)
  oNewDesignator = oNewDesignator or {}
  oNewDesignator.uOwner = self.uOwner
  oNewDesignator.bDesignateOnDeath = true
  oNewDesignator.bDesignationComplete = self.bDesignationComplete
  oNewDesignator.sDesignationType = "Flare Designator"
  oNewDesignator.fValidationFunction = MrxSupportDesignator.ValidateWaterDropZone
  oNewDesignator.tCallbackList = {}
  oNewDesignator.sAATestLevel = "none"
  oNewDesignator.nX = self.nX
  oNewDesignator.nY = self.nY
  oNewDesignator.nZ = self.nZ
  oNewDesignator.uGuid = self.uGuid
  setmetatable(oNewDesignator, self)
  self.__index = self
  oNewDesignator:AddCompleteCallback(DesignationCompleteCallback, {oNewDesignator})
  return oNewDesignator
end

function DesignationCompleteCallback(self)
  if self.uGuid then
    local nX, nY, nZ = Object.GetPosition(self.uGuid)
    if nX and nY and nZ then
      Airstrike.SpawnOrdnance("Flare Projectile Stage 2", nX, nY, nZ, 0, -2, 0)
    end
  end
end

function GetType(self)
  return "flare"
end
