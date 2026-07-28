inherit("MrxSupportDesignator")

function Create(self, oNewDesignator)
  oNewDesignator = oNewDesignator or {}
  oNewDesignator.uOwner = self.uOwner
  oNewDesignator.bDesignateOnDeath = false
  oNewDesignator.bDesignationComplete = self.bDesignationComplete
  oNewDesignator.sDesignationType = "Beacon Designator"
  oNewDesignator.fValidationFunction = nil
  oNewDesignator.tCallbackList = {}
  oNewDesignator.sAATestLevel = "jammer"
  oNewDesignator.nX = self.nX
  oNewDesignator.nY = self.nY
  oNewDesignator.nZ = self.nZ
  oNewDesignator.uGuid = self.uGuid
  setmetatable(oNewDesignator, self)
  self.__index = self
  return oNewDesignator
end

function GetType(self)
  return "beacon"
end
